//! srv-standard：服务端标准插件（cdylib）。
//!
//! 实现 terminal_chat 的服务端业务：频点（freq）管理、用户管理、
//! 消息（msg/reply/call@user）、远程文件系统。
//!
//! 通信方式：客户端把请求 proto 编码后 base64 放入 `Packet.payload`，
//! 以 `Packet` 帧发给服务器；服务器按 `packet.channel` 路由到本插件
//! 注册的 handler，handler 处理后把响应 proto 编码 + base64 放入
//! `PacketResult.payload`，由 srv 主程序写回客户端。
//!
//! 用户身份：`packet.sender` 由 srv 主程序规范化为 `user@ip` 形式。

use libloading::{Library, Symbol};
use prost::Message;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use srv_core_types::channels::ChannelMetadata;
use srv_core_types::packets::{Packet, PacketResult};
use srv_core_types::plugin_fn_types::{ChannelHandlerFn, SetChannelHandlerFn};
use srv_core_types::plugins::{SrvHostMetadata, SrvPluginMetadata, SrvPluginResult};
use srv_core_types::RegisterChannelFn;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// 服务端业务状态
// ---------------------------------------------------------------------------

/// 频点（frequency）：0~1 之间的小数（字符串保存），是聊天/通话的基本单位。
pub(crate) struct FreqRoom {
    name: String,
    password: Option<String>,
    max_members: i32,                    // 0 = 无限制
    creator: String,                     // user@ip
    members: HashMap<String, Member>,    // user@ip -> member
    banned: HashSet<String>,             // 被 ban 的 user（user@ip）
    no_mod: bool,                        // -n/--no-mod：无版主，创建者也可退出
    #[allow(dead_code)] // 元数据保留
    created_at: u64,
}

pub(crate) struct Member {
    user: String,       // user@ip
    nickname: String,
    is_moderator: bool,
    away: String,       // "" / "away" / "busy"
    muted_until: u64,   // 禁言截止（Unix 秒），0 = 未禁言
    #[allow(dead_code)] // 元数据保留
    joined_at: u64,
}

/// 用户档案（跨频点）。
pub(crate) struct UserProfile {
    user: String,       // user@ip
    nickname: String,
    online: bool,
    freq: String,       // 当前所在频点（"" = 未加入任何频点）
    away: String,       // "" / "away" / "busy"
    connected_at: u64,
    ignored: HashSet<String>,  // 忽略的用户（user@ip）
}

pub(crate) struct ServerState {
    freqs: HashMap<String, FreqRoom>,
    users: HashMap<String, UserProfile>,
    last_speaker: HashMap<String, String>, // freq -> 最近发言者 user@ip
}

/// 远程虚拟文件系统：user@ip -> 该用户的文件树。
pub(crate) struct VfsEntry {
    is_dir: bool,
    size: u64,
    content: Vec<u8>,
    modified_at: u64,
}

pub(crate) struct VfsUser {
    cwd: String,
    files: HashMap<String, VfsEntry>, // 绝对路径 -> 条目
}

pub(crate) static STATE: LazyLock<Mutex<ServerState>> = LazyLock::new(|| {
    Mutex::new(ServerState {
        freqs: HashMap::new(),
        users: HashMap::new(),
        last_speaker: HashMap::new(),
    })
});

pub(crate) static VFS: LazyLock<Mutex<HashMap<String, VfsUser>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 按用户（user@ip）排队的待读通知（文本），由 `notifications` RPC 拉取后清空。
pub(crate) static NOTIFICATIONS: LazyLock<Mutex<HashMap<String, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 向用户追加一条待读通知。
pub(crate) fn notify(target: &str, text: String) {
    NOTIFICATIONS
        .lock()
        .unwrap()
        .entry(target.to_string())
        .or_default()
        .push(text);
}

pub(crate) fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn b64e(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

fn b64d(s: &str) -> Option<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s).ok()
}

/// freq 是否合法：0~1 之间的小数（字符串保存）。
pub(crate) fn valid_freq(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 32 {
        return false;
    }
    match t.parse::<f64>() {
        Ok(v) => (0.0..=1.0).contains(&v),
        Err(_) => false,
    }
}

/// 用户名的 user 部分（去掉 @ip）。
pub(crate) fn user_part(full: &str) -> String {
    full.split('@').next().unwrap_or(full).to_string()
}

/// 展示用用户名：freq 内该用户名唯一时省略 @IP，否则显示完整 user@ip。
pub(crate) fn display_user(state: &ServerState, freq: &str, user: &str) -> String {
    let name = user_part(user);
    let room = state.freqs.get(freq);
    let same_name = room
        .map(|r| r.members.values().filter(|m| user_part(&m.user) == name).count())
        .unwrap_or(0);
    if same_name <= 1 {
        name
    } else {
        user.to_string()
    }
}

/// 把参数中的用户标识解析为完整 user@ip（#14：freq 内用户名唯一时允许省略 @IP）。
/// - 已含 '@'：原样返回（语义由调用方决定）。
/// - 裸名：先按 freq 成员匹配；无结果再按全局用户匹配；多于一个匹配视为歧义。
pub(crate) fn resolve_user(state: &ServerState, freq: &str, name: &str) -> Result<String, String> {
    let name_trim = name.trim();
    if name_trim.is_empty() {
        return Err("empty user".into());
    }
    if name_trim.contains('@') {
        return Ok(name_trim.to_string());
    }
    let mut cands: Vec<String> = Vec::new();
    if let Some(room) = state.freqs.get(freq) {
        for m in room.members.keys() {
            if user_part(m) == name_trim {
                cands.push(m.clone());
            }
        }
    }
    if cands.is_empty() {
        for u in state.users.keys() {
            if user_part(u) == name_trim {
                cands.push(u.clone());
            }
        }
    }
    match cands.len() {
        0 => Err(format!("user `{name}` not found")),
        1 => Ok(cands[0].clone()),
        _ => Err(format!(
            "user `{name}` is ambiguous ({}); use full user@ip",
            cands.join(", ")
        )),
    }
}

// ---------------------------------------------------------------------------
// RPC handler 基础设施
// ---------------------------------------------------------------------------

/// 取请求字节（base64 decode payload）。
fn request_bytes(p: &Packet) -> Result<Vec<u8>, String> {
    b64d(&p.payload).ok_or_else(|| "payload is not valid base64".to_string())
}

/// 构造带响应负载的结果。
fn respond(resp_bytes: &[u8], msg: impl Into<String>) -> PacketResult {
    PacketResult::handled_with(msg, b64e(resp_bytes))
}

/// 生成一个 RPC handler：decode 请求 -> 业务函数 -> encode 响应。
mod handlers;
use handlers::messaging::handle_send_message;
use handlers::messaging::handle_reply_message;
use handlers::messaging::handle_call_user;
use handlers::user::handle_user_info;
use handlers::user::handle_whoami;
use handlers::user::handle_ignore;
use handlers::user::handle_unignore;
use handlers::user::handle_set_presence;
use handlers::freq::handle_create_freq;
use handlers::freq::handle_join_freq;
use handlers::freq::handle_leave_freq;
use handlers::freq::handle_freq_info;
use handlers::freq::handle_disband_freq;
use handlers::freq::handle_active_freqs;
use handlers::freq::handle_active_users;
use handlers::freq::handle_moderators;
use handlers::moderation::handle_kick;
use handlers::moderation::handle_ban;
use handlers::moderation::handle_unban;
use handlers::moderation::handle_mute;
use handlers::moderation::handle_unmute;
use handlers::moderation::handle_op;
use handlers::moderation::handle_deop;
use handlers::moderation::handle_trop;
use handlers::fs::handle_remote_pwd;
use handlers::fs::handle_remote_ls;
use handlers::fs::handle_remote_cd;
use handlers::fs::handle_remote_mkdir;
use handlers::fs::handle_file_info;
use handlers::fs::handle_remote_checksum;
use handlers::fs::handle_upload;
use handlers::fs::handle_download;
use handlers::session::handle_status;
use handlers::session::handle_route;
use handlers::session::handle_notifications;
use handlers::session::handle_heartbeat;

macro_rules! rpc_handler {
    ($name:ident, $req_ty:ty, $resp_ty:ty, $handler:expr) => {
        extern "C" fn $name(packet: *const Packet) -> PacketResult {
            let p = unsafe { &*packet };
            let bytes = match request_bytes(p) {
                Ok(b) => b,
                Err(e) => return PacketResult::unhandled(format!("bad {} request: {e}", stringify!($name))),
            };
            let req = match <$req_ty as prost::Message>::decode(&bytes[..]) {
                Ok(r) => r,
                Err(e) => return PacketResult::unhandled(format!("bad {} request: {e}", stringify!($name))),
            };
            let resp: $resp_ty = $handler(p.sender.to_string(), req);
            respond(&resp.encode_to_vec(), format!("{} ok", stringify!($name)))
        }
    };
}

// ---------------------------------------------------------------------------
// 会话/用户辅助
// ---------------------------------------------------------------------------

/// 获取或初始化用户档案。
pub(crate) fn ensure_user(state: &mut ServerState, user: &str) {
    state.users.entry(user.to_string()).or_insert_with(|| UserProfile {
        user: user.to_string(),
        nickname: user_part(user),
        online: true,
        freq: String::new(),
        away: String::new(),
        connected_at: now_ts(),
        ignored: HashSet::new(),
    });
}

/// 取用户当前所在频点。
pub(crate) fn user_freq(state: &ServerState, user: &str) -> Option<String> {
    state.users.get(user).map(|u| u.freq.clone()).filter(|f| !f.is_empty())
}

// ---------------------------------------------------------------------------
// 消息：msg / reply / call(@user)
// ---------------------------------------------------------------------------




// ---------------------------------------------------------------------------
// 用户：user_info / whoami / ignore / unignore / away
// ---------------------------------------------------------------------------






// ---------------------------------------------------------------------------
// 频点：create / join / leave / info / disband / active 系列
// ---------------------------------------------------------------------------









// ---------------------------------------------------------------------------
// 频点管理：kick / ban / unban / mute / unmute / op / deop / trop
// ---------------------------------------------------------------------------

/// 校验操作者是否为频点创建者或管理员。
pub(crate) fn mod_auth(state: &ServerState, freq: &str, operator: &str) -> bool {
    match state.freqs.get(freq) {
        Some(r) => {
            r.creator == operator
                || r.members
                    .get(operator)
                    .map(|m| m.is_moderator)
                    .unwrap_or(false)
        }
        None => false,
    }
}









// ---------------------------------------------------------------------------
// 远程文件系统
// ---------------------------------------------------------------------------

/// 规范化路径：以 / 开头，解析 . 和 ..，不允许逃逸根目录。
pub(crate) fn norm_path(cwd: &str, path: &str) -> String {
    let combined = if path.starts_with('/') { path.to_string() } else { format!("{cwd}/{path}") };
    let mut parts: Vec<String> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s.to_string()),
        }
    }
    format!("/{}", parts.join("/"))
}

pub(crate) fn vfs_ensure_dirs(vfs: &mut VfsUser, path: &str, now: u64) {
    // 确保路径的所有父目录存在。
    let mut acc = String::new();
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        acc = format!("{acc}/{seg}");
        vfs.files.entry(acc.clone()).or_insert_with(|| VfsEntry {
            is_dir: true,
            size: 0,
            content: Vec::new(),
            modified_at: now,
        });
    }
}









// ---------------------------------------------------------------------------
// 会话：status（ping）/ route
// ---------------------------------------------------------------------------




// ---------------------------------------------------------------------------
// RPC handler 注册
// ---------------------------------------------------------------------------

rpc_handler!(on_send_message, protos::messaging::message::SendMessageRequest, protos::messaging::message::SendMessageResponse, handle_send_message);
rpc_handler!(on_reply_message, protos::messaging::message::ReplyMessageRequest, protos::messaging::message::ReplyMessageResponse, handle_reply_message);
rpc_handler!(on_call_user, protos::call::CallRequest, protos::call::CallResponse, handle_call_user);
rpc_handler!(on_user_info, protos::messaging::user::UserInfoRequest, protos::messaging::user::UserInfoResponse, handle_user_info);
rpc_handler!(on_whoami, protos::messaging::user::WhoamiRequest, protos::messaging::user::WhoamiResponse, handle_whoami);
rpc_handler!(on_ignore, protos::messaging::user::IgnoreRequest, protos::messaging::user::IgnoreResponse, handle_ignore);
rpc_handler!(on_unignore, protos::messaging::user::UnignoreRequest, protos::messaging::user::UnignoreResponse, handle_unignore);
rpc_handler!(on_set_presence, protos::connection::session::SetPresenceRequest, protos::connection::session::SetPresenceResponse, handle_set_presence);
rpc_handler!(on_create_freq, protos::frequency::basic::CreateFreqRequest, protos::frequency::basic::CreateFreqResponse, handle_create_freq);
rpc_handler!(on_join_freq, protos::frequency::basic::JoinFreqRequest, protos::frequency::basic::JoinFreqResponse, handle_join_freq);
rpc_handler!(on_leave_freq, protos::utils::empty::Empty, protos::frequency::basic::LeaveFreqResponse, handle_leave_freq);
rpc_handler!(on_freq_info, protos::frequency::basic::FreqInfoRequest, protos::frequency::basic::FreqInfoResponse, handle_freq_info);
rpc_handler!(on_disband_freq, protos::frequency::mod_ops::DisbandRequest, protos::frequency::mod_ops::DisbandResponse, handle_disband_freq);
rpc_handler!(on_active_freqs, protos::frequency::status::ActiveFreqsRequest, protos::frequency::status::ActiveFreqsResponse, handle_active_freqs);
rpc_handler!(on_active_users, protos::frequency::status::ActiveUsersRequest, protos::frequency::status::ActiveUsersResponse, handle_active_users);
rpc_handler!(on_moderators, protos::frequency::status::ModeratorsRequest, protos::frequency::status::ModeratorsResponse, handle_moderators);
rpc_handler!(on_kick, protos::frequency::mod_ops::KickRequest, protos::frequency::mod_ops::KickResponse, handle_kick);
rpc_handler!(on_ban, protos::frequency::mod_ops::BanRequest, protos::frequency::mod_ops::BanResponse, handle_ban);
rpc_handler!(on_unban, protos::frequency::mod_ops::UnbanRequest, protos::frequency::mod_ops::UnbanResponse, handle_unban);
rpc_handler!(on_mute, protos::frequency::mod_ops::MuteRequest, protos::frequency::mod_ops::MuteResponse, handle_mute);
rpc_handler!(on_unmute, protos::frequency::mod_ops::UnmuteRequest, protos::frequency::mod_ops::UnmuteResponse, handle_unmute);
rpc_handler!(on_op, protos::frequency::mod_ops::OpRequest, protos::frequency::mod_ops::OpResponse, handle_op);
rpc_handler!(on_deop, protos::frequency::mod_ops::DeopRequest, protos::frequency::mod_ops::DeopResponse, handle_deop);
rpc_handler!(on_trop, protos::frequency::mod_ops::TropRequest, protos::frequency::mod_ops::TropResponse, handle_trop);
rpc_handler!(on_remote_pwd, protos::remote::fs::PwdRequest, protos::remote::fs::PwdResponse, handle_remote_pwd);
rpc_handler!(on_remote_ls, protos::remote::fs::LsRequest, protos::remote::fs::LsResponse, handle_remote_ls);
rpc_handler!(on_remote_cd, protos::remote::fs::CdRequest, protos::remote::fs::CdResponse, handle_remote_cd);
rpc_handler!(on_remote_mkdir, protos::remote::fs::MkdirRequest, protos::remote::fs::MkdirResponse, handle_remote_mkdir);
rpc_handler!(on_file_info, protos::remote::fs::RemoteFileInfoRequest, protos::remote::fs::RemoteFileInfoResponse, handle_file_info);
rpc_handler!(on_remote_checksum, protos::remote::fs::RemoteChecksumRequest, protos::remote::fs::RemoteChecksumResponse, handle_remote_checksum);
rpc_handler!(on_upload, protos::remote::fs::UploadRequest, protos::remote::fs::UploadResponse, handle_upload);
rpc_handler!(on_download, protos::remote::fs::DownloadRequest, protos::remote::fs::DownloadResponse, handle_download);
rpc_handler!(on_status, protos::connection::session::StatusRequest, protos::connection::session::StatusResponse, handle_status);
rpc_handler!(on_route, protos::connection::session::RouteRequest, protos::connection::session::RouteResponse, handle_route);
rpc_handler!(on_notifications, protos::frequency::notification::NotificationsRequest, protos::frequency::notification::NotificationsResponse, handle_notifications);
rpc_handler!(on_heartbeat, protos::connection::session::HeartbeatRequest, protos::connection::session::HeartbeatResponse, handle_heartbeat);

// ---------------------------------------------------------------------------
// 插件生命周期
// ---------------------------------------------------------------------------

static METADATA: LazyLock<SrvPluginMetadata> = LazyLock::new(|| SrvPluginMetadata {
    name: "srv-standard".into(),
    dylib_name: "srv_standard".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "srv standard channels: freq / user / message / remote fs.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::Some("0.1.0".into()),
    max_host_version: TaggedOption::None,
});

static LIB: OnceLock<Library> = OnceLock::new();

fn init_library(path: &str) -> &'static Library {
    LIB.get_or_init(|| unsafe { Library::new(path).expect("Failed to load library") })
}

/// (channel 名, 元数据, handler)。
const CHANNELS: &[(&str, &str, ChannelHandlerFn)] = &[
    ("send_message", "Send a message to a freq (broadcast or directed)", on_send_message),
    ("reply_message", "Reply to the last speaker", on_reply_message),
    ("call_user", "Call (at) an user in a freq", on_call_user),
    ("user_info", "Show user information", on_user_info),
    ("whoami", "Show current user identity", on_whoami),
    ("ignore_user", "Ignore an user", on_ignore),
    ("unignore_user", "Unignore an user", on_unignore),
    ("set_presence", "Set away/busy presence", on_set_presence),
    ("create_freq", "Create a frequency channel", on_create_freq),
    ("join_freq", "Join a frequency channel", on_join_freq),
    ("leave_freq", "Leave current frequency", on_leave_freq),
    ("freq_info", "Show frequency information", on_freq_info),
    ("disband_freq", "Disband current frequency", on_disband_freq),
    ("active_freqs", "List active frequencies", on_active_freqs),
    ("active_users", "List active users in a freq", on_active_users),
    ("moderators", "List moderators in a freq", on_moderators),
    ("kick_user", "Kick an user from freq", on_kick),
    ("ban_user", "Ban an user from freq", on_ban),
    ("unban_user", "Unban an user", on_unban),
    ("mute_user", "Mute an user", on_mute),
    ("unmute_user", "Unmute an user", on_unmute),
    ("op_user", "Grant moderator to an user", on_op),
    ("deop_user", "Revoke moderator from an user", on_deop),
    ("trop_user", "Transfer creator permission", on_trop),
    ("remote_pwd", "Show remote working directory", on_remote_pwd),
    ("remote_ls", "List remote directory", on_remote_ls),
    ("remote_cd", "Change remote directory", on_remote_cd),
    ("remote_mkdir", "Make remote directory", on_remote_mkdir),
    ("file_info", "Remote file information", on_file_info),
    ("remote_checksum", "Remote file SHA256", on_remote_checksum),
    ("upload", "Upload a file to server", on_upload),
    ("download", "Download a file from server", on_download),
    ("status", "Server ping / network test", on_status),
    ("route", "Trace route to server", on_route),
    ("notifications", "Poll pending notifications", on_notifications),
    ("heartbeat", "Long-connection heartbeat, carries pending notifications", on_heartbeat),
];

fn channel_metadata(name: &str, desc: &str) -> ChannelMetadata {
    ChannelMetadata {
        name: name.into(),
        channel_type: srv_core_types::channels::channel_type::CHAT,
        description: desc.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
        capacity: -1,
    }
}

fn register_channel_with_handler(
    lib: &Library,
    metadata: &ChannelMetadata,
    handler: ChannelHandlerFn,
) {
    let register_channel: Symbol<RegisterChannelFn> = unsafe { lib.get(b"register_channel") }
        .expect("Failed to find symbol 'register_channel'");
    let set_channel_handler: Symbol<SetChannelHandlerFn> =
        unsafe { lib.get(b"set_channel_handler") }
            .expect("Failed to find symbol 'set_channel_handler'");

    let registered = register_channel(metadata);
    if registered == 0 {
        println!(
            "[srv-standard] channel '{}' already registered, skipping.",
            metadata.name
        );
        return;
    }
    let bound = set_channel_handler(&metadata.name, handler);
    if bound == 0 {
        println!(
            "[srv-standard] failed to bind handler for channel '{}'.",
            metadata.name
        );
    }
}

#[ffi_export]
pub fn get_plugin_metadata() -> SrvPluginMetadata {
    METADATA.clone()
}

#[ffi_export]
pub fn on_init_plugin(h_meta: SrvHostMetadata) -> SrvPluginResult {
    let lib = init_library(&h_meta.srv_core_path);

    for (name, desc, handler) in CHANNELS {
        register_channel_with_handler(lib, &channel_metadata(name, desc), *handler);
    }
    println!(
        "[srv-standard] registered {} channel(s).",
        CHANNELS.len()
    );

    SrvPluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> SrvPluginResult {
    SrvPluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}
