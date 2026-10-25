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
use sha2::{Digest, Sha256};
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
struct FreqRoom {
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

struct Member {
    user: String,       // user@ip
    nickname: String,
    is_moderator: bool,
    away: String,       // "" / "away" / "busy"
    muted_until: u64,   // 禁言截止（Unix 秒），0 = 未禁言
    #[allow(dead_code)] // 元数据保留
    joined_at: u64,
}

/// 用户档案（跨频点）。
struct UserProfile {
    user: String,       // user@ip
    nickname: String,
    online: bool,
    freq: String,       // 当前所在频点（"" = 未加入任何频点）
    away: String,       // "" / "away" / "busy"
    connected_at: u64,
    ignored: HashSet<String>,  // 忽略的用户（user@ip）
}

struct ServerState {
    freqs: HashMap<String, FreqRoom>,
    users: HashMap<String, UserProfile>,
    last_speaker: HashMap<String, String>, // freq -> 最近发言者 user@ip
}

/// 远程虚拟文件系统：user@ip -> 该用户的文件树。
struct VfsEntry {
    is_dir: bool,
    size: u64,
    content: Vec<u8>,
    modified_at: u64,
}

struct VfsUser {
    cwd: String,
    files: HashMap<String, VfsEntry>, // 绝对路径 -> 条目
}

static STATE: LazyLock<Mutex<ServerState>> = LazyLock::new(|| {
    Mutex::new(ServerState {
        freqs: HashMap::new(),
        users: HashMap::new(),
        last_speaker: HashMap::new(),
    })
});

static VFS: LazyLock<Mutex<HashMap<String, VfsUser>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 按用户（user@ip）排队的待读通知（文本），由 `notifications` RPC 拉取后清空。
static NOTIFICATIONS: LazyLock<Mutex<HashMap<String, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 向用户追加一条待读通知。
fn notify(target: &str, text: String) {
    NOTIFICATIONS
        .lock()
        .unwrap()
        .entry(target.to_string())
        .or_default()
        .push(text);
}

fn now_ts() -> u64 {
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
fn valid_freq(s: &str) -> bool {
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
fn user_part(full: &str) -> String {
    full.split('@').next().unwrap_or(full).to_string()
}

/// 展示用用户名：freq 内该用户名唯一时省略 @IP，否则显示完整 user@ip。
fn display_user(state: &ServerState, freq: &str, user: &str) -> String {
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
fn ensure_user(state: &mut ServerState, user: &str) {
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
fn user_freq(state: &ServerState, user: &str) -> Option<String> {
    state.users.get(user).map(|u| u.freq.clone()).filter(|f| !f.is_empty())
}

// ---------------------------------------------------------------------------
// 消息：msg / reply / call(@user)
// ---------------------------------------------------------------------------

fn handle_send_message(sender: String, req: protos::messaging::message::SendMessageRequest) -> protos::messaging::message::SendMessageResponse {
    use protos::messaging::message::{SendMessageResponse, SendMessageResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return SendMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: "you are not in any freq; use `join_freq` or `create_freq` first".into(),
            }
        }
    };
    // 被禁言检查。
    if let Some(member) = state.freqs.get(&freq).and_then(|f| f.members.get(&sender))
        && member.muted_until > now_ts()
    {
        return SendMessageResponse {
            result: R::SendMessageMuted as i32,
            message: "you are muted in this freq".into(),
        };
    }
    let targets: Vec<String> = req.users.iter().map(|u| u.to_string()).collect();
    if targets.is_empty() {
        let who = display_user(&state, &freq, &sender);
        println!(
            "[srv-standard] [{freq}] {who}: {}",
            req.message
        );
        state.last_speaker.insert(freq.clone(), sender.clone());
        SendMessageResponse {
            result: R::SendMessageSuccess as i32,
            message: format!("[{freq}] {who}: {}", req.message),
        }
    } else {
        // 定向发送：校验每个目标是否在频点内（不在则失败）。
        let room = state.freqs.get(&freq).expect("freq exists");
        for t in &targets {
            if !room.members.contains_key(t) {
                return SendMessageResponse {
                    result: R::SendMessageUserNotFound as i32,
                    message: format!("user `{t}` is not in freq `{freq}`"),
                };
            }
        }
        let who = display_user(&state, &freq, &sender);
        println!(
            "[srv-standard] [{freq}] {who} -> {}: {}",
            targets.join(","),
            req.message
        );
        // 通知每个定向目标（在线的用户稍后由 cli 轮询拉取）。
        for t in &targets {
            if t.as_str() != sender {
                notify(t, format!("[msg:{freq}] {who}: {}", req.message));
            }
        }
        state.last_speaker.insert(freq, sender);
        SendMessageResponse {
            result: R::SendMessageSuccess as i32,
            message: format!("-> {}: {}", targets.join(","), req.message),
        }
    }
}

fn handle_reply_message(sender: String, req: protos::messaging::message::ReplyMessageRequest) -> protos::messaging::message::ReplyMessageResponse {
    use protos::messaging::message::{ReplyMessageResponse, SendMessageResult as R};
    let state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return ReplyMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: "you are not in any freq".into(),
            }
        }
    };
    let target = match state.last_speaker.get(&freq) {
        Some(t) if t.as_str() != sender => t.clone(),
        _ => {
            return ReplyMessageResponse {
                result: R::SendMessageError as i32,
                message: "no one to reply to in this freq".into(),
            }
        }
    };
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!(
        "[srv-standard] [{freq}] {who} -> {target_disp}: {}",
        req.message
    );
    ReplyMessageResponse {
        result: R::SendMessageSuccess as i32,
        message: format!("-> {target_disp}: {}", req.message),
    }
}

fn handle_call_user(sender: String, req: protos::call::CallRequest) -> protos::call::CallResponse {
    use protos::call::{CallResponse, CallResult as R};
    let state = STATE.lock().unwrap();
    let target = req.user;
    // 目标必须存在且在线。
    let profile = match state.users.get(&target) {
        Some(u) => u,
        None => {
            return CallResponse {
                result: R::CallUserNotFound as i32,
                message: format!("user `{target}` not found"),
            }
        }
    };
    if !profile.online {
        return CallResponse {
            result: R::CallUserOffline as i32,
            message: format!("user `{target}` is offline"),
        };
    }
    if !profile.away.is_empty() {
        return CallResponse {
            result: R::CallUserBusy as i32,
            message: format!("user `{target}` is {} (auto-reply: contact me later)", profile.away),
        };
    }
    let freq = profile.freq.clone();
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!(
        "[srv-standard] CALL {who} -> {target_disp} ({freq}) @{}: {}",
        req.freq,
        req.message
    );
    // 呼叫即通知目标用户（打印到服务端日志，等价于 @user 提醒）。
    let msg = if req.message.is_empty() {
        format!("[call] {who} is calling you in {freq} (ring ring)")
    } else {
        format!("[call] {who} @{freq}: {}", req.message)
    };
    println!("[srv-standard] @{target} -> {msg}");
    notify(&target, msg.clone());
    CallResponse {
        result: R::CallRinging as i32,
        message: format!("calling {target} ... {msg}"),
    }
}

// ---------------------------------------------------------------------------
// 用户：user_info / whoami / ignore / unignore / away
// ---------------------------------------------------------------------------

fn handle_user_info(sender: String, req: protos::messaging::user::UserInfoRequest) -> protos::messaging::user::UserInfoResponse {
    use protos::messaging::user::{UserInfo, UserInfoResponse};
    let state = STATE.lock().unwrap();
    match state.users.get(&req.user) {
        Some(u) => {
            let info = UserInfo {
                user: u.user.clone(),
                nickname: u.nickname.clone(),
                ip_address: u.user.rsplit('@').next().unwrap_or("").to_string(),
                online: u.online,
                freq: u.freq.clone(),
                connected_at: u.connected_at as i64,
                is_moderator: u
                    .freq
                    .as_str()
                    .is_empty()
                    .not()
                    .then(|| state.freqs.get(&u.freq).map(|f| f.members.get(&u.user).map(|m| m.is_moderator).unwrap_or(false)).unwrap_or(false))
                    .unwrap_or(false),
                is_creator: u.freq.as_str().is_empty().not()
                    .then(|| state.freqs.get(&u.freq).map(|f| f.creator == u.user).unwrap_or(false))
                    .unwrap_or(false),
                ignored_you: u.ignored.contains(&sender),
            };
            UserInfoResponse {
                found: true,
                info: Some(info),
                message: "found".into(),
            }
        }
        None => UserInfoResponse { found: false, info: None, message: "user not found".into() },
    }
}

fn handle_whoami(sender: String, _req: protos::messaging::user::WhoamiRequest) -> protos::messaging::user::WhoamiResponse {
    use protos::messaging::user::WhoamiResponse;
    WhoamiResponse {
        user: user_part(&sender),
        full_user: sender.clone(),
        success: true,
        message: format!("you are {}", sender),
    }
}

fn handle_ignore(sender: String, req: protos::messaging::user::IgnoreRequest) -> protos::messaging::user::IgnoreResponse {
    use protos::messaging::user::IgnoreResponse;
    let mut state = STATE.lock().unwrap();
    if !state.users.contains_key(&req.user) {
        return IgnoreResponse { success: false, message: format!("user `{}` not found", req.user) };
    }
    ensure_user(&mut state, &sender);
    state.users.get_mut(&sender).unwrap().ignored.insert(req.user.clone());
    IgnoreResponse { success: true, message: format!("you are now ignoring {}", req.user) }
}

fn handle_unignore(sender: String, req: protos::messaging::user::UnignoreRequest) -> protos::messaging::user::UnignoreResponse {
    use protos::messaging::user::UnignoreResponse;
    let mut state = STATE.lock().unwrap();
    let removed = state
        .users
        .get_mut(&sender)
        .map(|u| u.ignored.remove(&req.user))
        .unwrap_or(false);
    UnignoreResponse {
        success: removed,
        message: if removed {
            format!("you are no longer ignoring {}", req.user)
        } else {
            format!("you were not ignoring {}", req.user)
        },
    }
}

fn handle_set_presence(sender: String, req: protos::connection::session::SetPresenceRequest) -> protos::connection::session::SetPresenceResponse {
    use protos::connection::session::{PresenceStatus, SetPresenceResponse};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    let away = match req.status() {
        PresenceStatus::PresenceAway => "away",
        PresenceStatus::PresenceBusy => "busy",
        _ => "",
    };
    state.users.get_mut(&sender).unwrap().away = away.to_string();
    if let Some(freq) = user_freq(&state, &sender)
        && let Some(m) = state.freqs.get_mut(&freq).and_then(|f| f.members.get_mut(&sender))
    {
        m.away = away.to_string();
    }
    SetPresenceResponse {
        success: true,
        status: req.status,
        message: format!("presence set to `{}`", if away.is_empty() { "online" } else { away }),
    }
}

// ---------------------------------------------------------------------------
// 频点：create / join / leave / info / disband / active 系列
// ---------------------------------------------------------------------------

fn handle_create_freq(sender: String, req: protos::frequency::basic::CreateFreqRequest) -> protos::frequency::basic::CreateFreqResponse {
    use protos::frequency::basic::{CreateFreqResponse, CreateResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateInvalidParam as i32,
            message: format!("`{}` is not a valid freq: must be a decimal in [0, 1]", req.freq),
        };
    }
    if state.freqs.contains_key(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateDuplicate as i32,
            message: format!("freq `{}` already exists", req.freq),
        };
    }
    let now = now_ts();
    state.freqs.insert(
        req.freq.clone(),
        FreqRoom {
            name: req.freq.clone(),
            password: req.password.clone(),
            max_members: req.max_people.unwrap_or(0),
            creator: sender.clone(),
            members: HashMap::new(),
            banned: HashSet::new(),
            no_mod: req.no_mod,
            #[allow(dead_code)]
            created_at: now,
        },
    );
    state.users.get_mut(&sender).unwrap().freq = req.freq.clone();
    // 创建者自动成为第一个成员（且是 moderator 语义上的 creator）。
    state.freqs.get_mut(&req.freq).unwrap().members.insert(
        sender.clone(),
        Member {
            user: sender.clone(),
            nickname: user_part(&sender),
            is_moderator: true,
            away: String::new(),
            muted_until: 0,
            joined_at: now,
        },
    );
    println!("[srv-standard] freq `{}` created by {} ({} joined)", req.freq, user_part(&sender), user_part(&sender));
    CreateFreqResponse {
        result: R::CreateSuccess as i32,
        message: format!("freq `{}` created, joined as creator", req.freq),
    }
}

fn handle_join_freq(sender: String, req: protos::frequency::basic::JoinFreqRequest) -> protos::frequency::basic::JoinFreqResponse {
    use protos::frequency::basic::{JoinFreqResponse, FreqStatus as S};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return JoinFreqResponse {
            status: S::FreqFormatWrong as i32,
            message: format!("`{}` is not a valid freq: must be a decimal in [0, 1]", req.freq),
        };
    }
    let room = match state.freqs.get_mut(&req.freq) {
        Some(r) => r,
        None => {
            return JoinFreqResponse {
                status: S::FreqError as i32,
                message: format!("freq `{}` does not exist; use `create_freq` first", req.freq),
            }
        }
    };
    if room.banned.contains(&sender) {
        return JoinFreqResponse {
            status: S::FreqError as i32,
            message: "you are banned from this freq".into(),
        };
    }
    if let Some(pw) = &room.password {
        if req.password.as_deref() != Some(pw.as_str()) {
            return JoinFreqResponse {
                status: S::FreqNeedPassword as i32,
                message: "wrong password for this freq".into(),
            };
        }
    }
    if room.max_members > 0 && room.members.len() as i32 >= room.max_members {
        return JoinFreqResponse {
            status: S::FreqFull as i32,
            message: format!("freq `{}` is full ({})", req.freq, room.max_members),
        };
    }
    if room.members.contains_key(&sender) {
        return JoinFreqResponse {
            status: S::FreqIdle as i32,
            message: format!("already in freq `{}`", req.freq),
        };
    }
    let now = now_ts();
    let count;
    {
        let room = state.freqs.get_mut(&req.freq).expect("checked above");
        room.members.insert(
            sender.clone(),
            Member {
                user: sender.clone(),
                nickname: user_part(&sender),
                is_moderator: false,
                away: String::new(),
                muted_until: 0,
                joined_at: now,
            },
        );
        count = room.members.len();
    }
    state.users.get_mut(&sender).unwrap().freq = req.freq.clone();
    println!(
        "[srv-standard] {} joined freq `{}` (members={})",
        user_part(&sender),
        req.freq,
        count
    );
    JoinFreqResponse {
        status: S::FreqIdle as i32,
        message: format!("joined freq `{}` (members={count})", req.freq),
    }
}

fn handle_leave_freq(sender: String, _req: protos::utils::empty::Empty) -> protos::frequency::basic::LeaveFreqResponse {
    use protos::frequency::basic::{LeaveFreqResponse, LeaveResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return LeaveFreqResponse {
                result: R::LeaveNotJoined as i32,
                message: "you are not in any freq".into(),
            }
        }
    };
    // 创建者不能直接离开（除非 -n/--no-mod 无版主频点）。
    let no_mod = state.freqs.get(&freq).map(|f| f.no_mod).unwrap_or(false);
    if state.freqs.get(&freq).map(|f| f.creator == sender).unwrap_or(false) && !no_mod {
        return LeaveFreqResponse {
            result: R::LeaveFailed as i32,
            message: "you are the creator; use `disband` to remove the freq".into(),
        };
    }
    if let Some(room) = state.freqs.get_mut(&freq) {
        room.members.remove(&sender);
    }
    state.users.get_mut(&sender).unwrap().freq = String::new();
    // 无版主频点的创建者离开后，若不再有人则自动解散。
    if no_mod && state.freqs.get(&freq).map(|f| f.members.is_empty()).unwrap_or(false) {
        state.freqs.remove(&freq);
        VFS.lock().unwrap().remove(&freq);
        println!("[srv-standard] freq `{freq}` auto-disbanded (no_mod creator left)");
        return LeaveFreqResponse {
            result: R::LeaveSuccess as i32,
            message: format!("left freq `{freq}` (auto-disbanded)"),
        };
    }
    println!("[srv-standard] {} left freq `{}`", user_part(&sender), freq);
    LeaveFreqResponse {
        result: R::LeaveSuccess as i32,
        message: format!("left freq `{freq}`"),
    }
}

fn handle_freq_info(_sender: String, req: protos::frequency::basic::FreqInfoRequest) -> protos::frequency::basic::FreqInfoResponse {
    use protos::frequency::basic::{FreqInfo, FreqInfoResponse, MemberInfo};
    let state = STATE.lock().unwrap();
    match state.freqs.get(&req.freq) {
        Some(room) => {
            let members = room
                .members
                .values()
                .map(|m| MemberInfo {
                    nickname: m.nickname.clone(),
                    ip_address: m.user.rsplit('@').next().unwrap_or("").to_string(),
                })
                .collect();
            let info = FreqInfo {
                freq: room.name.clone(),
                has_password: room.password.is_some(),
                current_members: room.members.len() as i32,
                max_members: room.max_members,
                creator_nickname: user_part(&room.creator),
                creator_ip: room.creator.rsplit('@').next().unwrap_or("").to_string(),
                members,
            };
            FreqInfoResponse { found: true, info: Some(info), message: "found".into() }
        }
        None => FreqInfoResponse { found: false, info: None, message: format!("freq `{}` not found", req.freq) },
    }
}

fn handle_disband_freq(sender: String, req: protos::frequency::mod_ops::DisbandRequest) -> protos::frequency::mod_ops::DisbandResponse {
    use protos::frequency::mod_ops::{DisbandResponse, DisbandResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return DisbandResponse {
                    result: R::DisbandNotFound as i32,
                    message: "you are not in any freq".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return DisbandResponse {
                result: R::DisbandNotFound as i32,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    if room.creator != sender {
        return DisbandResponse {
            result: R::DisbandAccessDenied as i32,
            message: "only the creator can disband the freq".into(),
        };
    }
    if room.members.len() > 1 {
        return DisbandResponse {
            result: R::DisbandHasMembers as i32,
            message: format!("freq `{freq}` still has {} members", room.members.len()),
        };
    }
    state.freqs.remove(&freq);
    state.users.get_mut(&sender).unwrap().freq = String::new();
    // 频点销毁时同时销毁其共享 VFS（freq 内共享，无私有文件系统）。
    VFS.lock().unwrap().remove(&freq);
    println!("[srv-standard] freq `{freq}` disbanded by {}", user_part(&sender));
    DisbandResponse {
        result: R::DisbandSuccess as i32,
        message: format!("freq `{freq}` disbanded"),
    }
}

fn handle_active_freqs(_sender: String, _req: protos::frequency::status::ActiveFreqsRequest) -> protos::frequency::status::ActiveFreqsResponse {
    use protos::frequency::status::{ActiveFreq, ActiveFreqsResponse};
    let state = STATE.lock().unwrap();
    let freqs = state
        .freqs
        .values()
        .map(|r| ActiveFreq {
            freq: r.name.clone(),
            current_members: r.members.len() as i32,
            max_members: r.max_members,
            has_password: r.password.is_some(),
            creator: user_part(&r.creator),
        })
        .collect();
    ActiveFreqsResponse {
        freqs,
        success: true,
        message: format!("{} active freq(s)", state.freqs.len()),
    }
}

fn handle_active_users(sender: String, req: protos::frequency::status::ActiveUsersRequest) -> protos::frequency::status::ActiveUsersResponse {
    use protos::frequency::status::{ActiveUser, ActiveUsersResponse};
    let state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return ActiveUsersResponse {
                    freq: String::new(),
                    users: vec![],
                    success: false,
                    message: "you are not in any freq; specify one".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return ActiveUsersResponse {
                freq: freq.clone(),
                users: vec![],
                success: false,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    let users = room
        .members
        .values()
        .map(|m| ActiveUser {
            user: m.user.clone(),
            nickname: m.nickname.clone(),
            is_moderator: m.is_moderator,
            is_creator: room.creator == m.user,
            away: !m.away.is_empty(),
        })
        .collect();
    ActiveUsersResponse {
        freq: freq.clone(),
        users,
        success: true,
        message: format!("{} user(s) in freq `{freq}`", room.members.len()),
    }
}

fn handle_moderators(sender: String, req: protos::frequency::status::ModeratorsRequest) -> protos::frequency::status::ModeratorsResponse {
    use protos::frequency::status::{ActiveUser, ModeratorsResponse};
    let state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return ModeratorsResponse {
                    freq: String::new(),
                    moderators: vec![],
                    success: false,
                    message: "you are not in any freq; specify one".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return ModeratorsResponse {
                freq: freq.clone(),
                moderators: vec![],
                success: false,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    let mods = room
        .members
        .values()
        .filter(|m| m.is_moderator || room.creator == m.user)
        .map(|m| ActiveUser {
            user: m.user.clone(),
            nickname: m.nickname.clone(),
            is_moderator: m.is_moderator,
            is_creator: room.creator == m.user,
            away: !m.away.is_empty(),
        })
        .collect::<Vec<_>>();
    let n = mods.len();
    ModeratorsResponse {
        freq: freq.clone(),
        moderators: mods,
        success: true,
        message: format!("{n} moderator(s) in freq `{freq}`"),
    }
}

// ---------------------------------------------------------------------------
// 频点管理：kick / ban / unban / mute / unmute / op / deop / trop
// ---------------------------------------------------------------------------

/// 校验操作者是否为频点创建者或管理员。
fn mod_auth(state: &ServerState, freq: &str, operator: &str) -> bool {
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

fn handle_kick(sender: String, req: protos::frequency::mod_ops::KickRequest) -> protos::frequency::mod_ops::KickResponse {
    use protos::frequency::mod_ops::{KickResponse, KickResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return KickResponse { result: R::KickError as i32, message: "you are not in any freq".into() }
            }
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return KickResponse { result: R::KickUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return KickResponse { result: R::KickAccessDenied as i32, message: "you are not a moderator".into() };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == req.user {
            return KickResponse { result: R::KickCannotKickCreator as i32, message: "cannot kick the creator".into() };
        }
        if room.members.remove(&req.user).is_none() {
            return KickResponse { result: R::KickUserNotFound as i32, message: format!("user `{}` not in freq", req.user) };
        }
    }
    state.users.get_mut(&req.user).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} kicked {}", user_part(&sender), user_part(&req.user));
    notify(&req.user, format!("[kick:{freq}] you were kicked by {}", display_user(&state, &freq, &sender)));
    KickResponse { result: R::KickSuccess as i32, message: format!("kicked {} from {freq}", req.user) }
}

fn handle_ban(sender: String, req: protos::frequency::mod_ops::BanRequest) -> protos::frequency::mod_ops::BanResponse {
    use protos::frequency::mod_ops::{BanResponse, BanResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return BanResponse { result: R::BanError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return BanResponse { result: R::BanUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return BanResponse { result: R::BanAccessDenied as i32, message: "you are not a moderator".into() };
    }
    if !state.users.contains_key(&req.user) {
        return BanResponse { result: R::BanUserNotFound as i32, message: format!("user `{}` not found", req.user) };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == req.user {
            return BanResponse { result: R::BanCannotBanCreator as i32, message: "cannot ban the creator".into() };
        }
        room.banned.insert(req.user.clone());
        room.members.remove(&req.user);
    }
    state.users.get_mut(&req.user).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} banned {}", user_part(&sender), user_part(&req.user));
    notify(&req.user, format!("[ban:{freq}] you were banned by {}", display_user(&state, &freq, &sender)));
    BanResponse { result: R::BanSuccess as i32, message: format!("banned {} from {freq}", req.user) }
}

fn handle_unban(sender: String, req: protos::frequency::mod_ops::UnbanRequest) -> protos::frequency::mod_ops::UnbanResponse {
    use protos::frequency::mod_ops::{UnbanResponse, UnbanResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return UnbanResponse { result: R::UnbanError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return UnbanResponse { result: R::UnbanUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnbanResponse { result: R::UnbanAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.banned.remove(&req.user) {
        UnbanResponse { result: R::UnbanSuccess as i32, message: format!("unbanned {} from {freq}", req.user) }
    } else {
        UnbanResponse { result: R::UnbanUserNotFound as i32, message: format!("user `{}` is not banned", req.user) }
    }
}

fn handle_mute(sender: String, req: protos::frequency::mod_ops::MuteRequest) -> protos::frequency::mod_ops::MuteResponse {
    use protos::frequency::mod_ops::{MuteResponse, MuteResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return MuteResponse { result: R::MuteError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return MuteResponse { result: R::MuteUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return MuteResponse { result: R::MuteAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.creator == req.user {
        return MuteResponse { result: R::MuteCannotMuteCreator as i32, message: "cannot mute the creator".into() };
    }
    let member = match room.members.get_mut(&req.user) {
        Some(m) => m,
        None => return MuteResponse { result: R::MuteUserNotFound as i32, message: format!("user `{}` not in freq", req.user) },
    };
    member.muted_until = match req.duration {
        Some(d) if d >= 0 => now_ts() + d as u64,
        _ => u64::MAX, // -1 / 缺省：永久
    };
    println!("[srv-standard] {freq}: {} muted {}", user_part(&sender), user_part(&req.user));
    notify(&req.user, format!("[mute:{freq}] you were muted by {}", display_user(&state, &freq, &sender)));
    MuteResponse { result: R::MuteSuccess as i32, message: format!("muted {} in {freq}", req.user) }
}

fn handle_unmute(sender: String, req: protos::frequency::mod_ops::UnmuteRequest) -> protos::frequency::mod_ops::UnmuteResponse {
    use protos::frequency::mod_ops::{UnmuteResponse, UnmuteResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return UnmuteResponse { result: R::UnmuteError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return UnmuteResponse { result: R::UnmuteError as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnmuteResponse { result: R::UnmuteAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    let member = match room.members.get_mut(&req.user) {
        Some(m) => m,
        None => return UnmuteResponse { result: R::UnmuteUserNotMuted as i32, message: format!("user `{}` not in freq", req.user) },
    };
    if member.muted_until == 0 {
        return UnmuteResponse { result: R::UnmuteUserNotMuted as i32, message: format!("user `{}` is not muted", req.user) };
    }
    member.muted_until = 0;
    UnmuteResponse { result: R::UnmuteSuccess as i32, message: format!("unmuted {} in {freq}", req.user) }
}

fn handle_op(sender: String, req: protos::frequency::mod_ops::OpRequest) -> protos::frequency::mod_ops::OpResponse {
    use protos::frequency::mod_ops::{OpResponse, OpResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return OpResponse { result: R::OpError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return OpResponse { result: R::OpUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    // 只有创建者可授权。
    if room.creator != sender {
        return OpResponse { result: R::OpAccessDenied as i32, message: "only the creator can grant mod".into() };
    }
    let member = match room.members.get_mut(&req.user) {
        Some(m) => m,
        None => return OpResponse { result: R::OpUserNotFound as i32, message: format!("user `{}` not in freq", req.user) },
    };
    if member.is_moderator {
        return OpResponse { result: R::OpAlreadyModerator as i32, message: format!("{} is already a moderator", req.user) };
    }
    member.is_moderator = true;
    println!("[srv-standard] {freq}: {} opped {}", user_part(&sender), user_part(&req.user));
    OpResponse { result: R::OpSuccess as i32, message: format!("{} is now a moderator of {freq}", req.user) }
}

fn handle_deop(sender: String, req: protos::frequency::mod_ops::DeopRequest) -> protos::frequency::mod_ops::DeopResponse {
    use protos::frequency::mod_ops::{DeopResponse, DeopResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return DeopResponse { result: R::DeopError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return DeopResponse { result: R::DeopUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    if room.creator != sender {
        return DeopResponse { result: R::DeopAccessDenied as i32, message: "only the creator can revoke mod".into() };
    }
    if room.creator == req.user {
        return DeopResponse { result: R::DeopCannotDeopCreator as i32, message: "cannot deop the creator".into() };
    }
    let member = match room.members.get_mut(&req.user) {
        Some(m) => m,
        None => return DeopResponse { result: R::DeopUserNotFound as i32, message: format!("user `{}` not in freq", req.user) },
    };
    if !member.is_moderator {
        return DeopResponse { result: R::DeopNotModerator as i32, message: format!("{} is not a moderator", req.user) };
    }
    member.is_moderator = false;
    DeopResponse { result: R::DeopSuccess as i32, message: format!("{} is no longer a moderator of {freq}", req.user) }
}

fn handle_trop(sender: String, req: protos::frequency::mod_ops::TropRequest) -> protos::frequency::mod_ops::TropResponse {
    use protos::frequency::mod_ops::{TropResponse, TropResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return TropResponse { result: R::TropError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return TropResponse { result: R::TropUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    if room.creator != sender {
        return TropResponse { result: R::TropAccessDenied as i32, message: "only the creator can transfer ownership".into() };
    }
    if room.creator == req.user {
        return TropResponse { result: R::TropSelfTransfer as i32, message: "you are already the creator".into() };
    }
    if !room.members.contains_key(&req.user) {
        return TropResponse { result: R::TropUserNotFound as i32, message: format!("user `{}` not in freq", req.user) };
    }
    room.creator = req.user.clone();
    if let Some(m) = room.members.get_mut(&req.user) {
        m.is_moderator = true;
    }
    println!("[srv-standard] {freq}: creator transferred to {}", user_part(&req.user));
    TropResponse { result: R::TropSuccess as i32, message: format!("{} is now the creator of {freq}", req.user) }
}

// ---------------------------------------------------------------------------
// 远程文件系统
// ---------------------------------------------------------------------------

/// 规范化路径：以 / 开头，解析 . 和 ..，不允许逃逸根目录。
fn norm_path(cwd: &str, path: &str) -> String {
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

fn vfs_ensure_dirs(vfs: &mut VfsUser, path: &str, now: u64) {
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

fn handle_remote_pwd(sender: String, _req: protos::remote::fs::PwdRequest) -> protos::remote::fs::PwdResponse {
    use protos::remote::fs::PwdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return PwdResponse { directory: String::new(), success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    PwdResponse {
        directory: vfs.cwd.clone(),
        success: true,
        message: vfs.cwd.clone(),
    }
}

fn handle_remote_ls(sender: String, _req: protos::remote::fs::LsRequest) -> protos::remote::fs::LsResponse {
    use protos::remote::fs::{FileEntry, LsResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return LsResponse { directory: String::new(), entries: vec![], success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let mut entries = Vec::new();
    let prefix = if vfs.cwd == "/" { String::new() } else { vfs.cwd.clone() };
    for (path, entry) in &vfs.files {
        if !path.starts_with(&format!("{prefix}/")) && path != &vfs.cwd {
            continue;
        }
        let rest = &path[prefix.len()..];
        if rest.is_empty() || rest == "/" {
            continue;
        }
        let rest = rest.trim_start_matches('/');
        if rest.contains('/') {
            continue; // 只列直接子项
        }
        entries.push(FileEntry {
            name: rest.to_string(),
            is_dir: entry.is_dir,
            size: entry.size as i64,
            modified_at: entry.modified_at as i64,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    LsResponse {
        directory: vfs.cwd.clone(),
        entries,
        success: true,
        message: format!("listing {}", vfs.cwd),
    }
}

fn handle_remote_cd(sender: String, req: protos::remote::fs::CdRequest) -> protos::remote::fs::CdResponse {
    use protos::remote::fs::CdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return CdResponse { directory: String::new(), success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.dir);
    // 根目录 "/" 总是存在（无对应 VFS 条目也视为合法目录）。
    let exists = target == "/" || vfs.files.get(&target).map(|e| e.is_dir).unwrap_or(false);
    if !exists {
        return CdResponse {
            directory: vfs.cwd.clone(),
            success: false,
            message: format!("`{target}`: no such directory"),
        };
    }
    vfs.cwd = target.clone();
    CdResponse {
        directory: target.clone(),
        success: true,
        message: format!("now at {target}"),
    }
}

fn handle_remote_mkdir(sender: String, req: protos::remote::fs::MkdirRequest) -> protos::remote::fs::MkdirResponse {
    use protos::remote::fs::MkdirResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return MkdirResponse { success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let mut vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.directory);
    if vfs.files.contains_key(&target) {
        return MkdirResponse { success: false, message: format!("`{target}` already exists") };
    }
    let now = now_ts();
    vfs_ensure_dirs(&mut vfs, &target, now);
    MkdirResponse { success: true, message: format!("created directory {target}") }
}

fn handle_file_info(sender: String, req: protos::remote::fs::RemoteFileInfoRequest) -> protos::remote::fs::RemoteFileInfoResponse {
    use protos::remote::fs::{RemoteFileInfo, RemoteFileInfoResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteFileInfoResponse { found: false, info: None, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.file);
    match vfs.files.get(&target) {
        Some(e) => RemoteFileInfoResponse {
            found: true,
            info: Some(RemoteFileInfo {
                path: target.clone(),
                size: e.size as i64,
                modified_at: e.modified_at as i64,
                is_dir: e.is_dir,
            }),
            message: target.clone(),
        },
        None => RemoteFileInfoResponse {
            found: false,
            info: None,
            message: format!("`{target}` not found"),
        },
    }
}

fn handle_remote_checksum(sender: String, req: protos::remote::fs::RemoteChecksumRequest) -> protos::remote::fs::RemoteChecksumResponse {
    use protos::remote::fs::RemoteChecksumResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteChecksumResponse { checksum: String::new(), found: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.file);
    match vfs.files.get(&target) {
        Some(e) if !e.is_dir => {
            let digest = Sha256::digest(&e.content);
            RemoteChecksumResponse {
                checksum: format!("{digest:x}"),
                found: true,
                message: target.clone(),
            }
        }
        Some(_) => RemoteChecksumResponse { checksum: String::new(), found: false, message: "cannot checksum a directory".into() },
        None => RemoteChecksumResponse { checksum: String::new(), found: false, message: format!("`{target}` not found") },
    }
}

fn handle_upload(sender: String, req: protos::remote::fs::UploadRequest) -> protos::remote::fs::UploadResponse {
    use protos::remote::fs::UploadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return UploadResponse { remote_path: String::new(), received_bytes: 0, success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let mut vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.remote_path);
    let now = now_ts();
    vfs_ensure_dirs(&mut vfs, &target, now);
    let name = target.rsplit('/').next().unwrap_or("file").to_string();
    let dir = target.trim_end_matches(&format!("/{name}"));
    if !dir.is_empty() {
        vfs_ensure_dirs(&mut vfs, dir, now);
    }
    vfs.files.insert(target.clone(), VfsEntry {
        is_dir: false,
        size: req.content.len() as u64,
        content: req.content.clone(),
        modified_at: now,
    });
    UploadResponse {
        remote_path: target.clone(),
        received_bytes: req.content.len() as i64,
        success: true,
        message: format!("uploaded {} bytes to {target}", req.content.len()),
    }
}

fn handle_download(sender: String, req: protos::remote::fs::DownloadRequest) -> protos::remote::fs::DownloadResponse {
    use protos::remote::fs::DownloadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.remote_path);
    match vfs.files.get(&target) {
        Some(e) if !e.is_dir => DownloadResponse {
            total_size: e.size as i64,
            found: true,
            content: e.content.clone(),
            message: format!("{} bytes at {target}", e.size),
        },
        Some(_) => DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: "cannot download a directory".into() },
        None => DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: format!("`{target}` not found") },
    }
}

// ---------------------------------------------------------------------------
// 会话：status（ping）/ route
// ---------------------------------------------------------------------------

fn handle_status(sender: String, req: protos::connection::session::StatusRequest) -> protos::connection::session::StatusResponse {
    use protos::connection::session::StatusResponse;
    StatusResponse {
        nonce: req.nonce,
        rtt_ms: 1,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        message: format!("pong from srv-standard (hi {sender})"),
    }
}

fn handle_route(_sender: String, _req: protos::connection::session::RouteRequest) -> protos::connection::session::RouteResponse {
    use protos::connection::session::RouteResponse;
    RouteResponse {
        server: "srv-standard".to_string(),
        hops: vec![],
        success: true,
        message: "direct connection (1 hop)".to_string(),
    }
}

fn handle_notifications(sender: String, _req: protos::frequency::notification::NotificationsRequest) -> protos::frequency::notification::NotificationsResponse {
    use protos::frequency::notification::{NotificationMessage, NotificationsResponse};
    let now = now_ts();
    let mut all = NOTIFICATIONS.lock().unwrap();
    let items = all
        .remove(&sender)
        .unwrap_or_default()
        .into_iter()
        .map(|text| {
            let (kind, rest) = text.split_once(':').unwrap_or(("msg", text.as_str()));
            NotificationMessage {
                kind: kind.trim_start_matches('[').to_string(),
                from: String::new(),
                text: rest.to_string(),
                at: now as i64,
            }
        })
        .collect();
    NotificationsResponse {
        success: true,
        items,
        message: "notifications".into(),
    }
}

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
