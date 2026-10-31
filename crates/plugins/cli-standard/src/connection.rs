//! TCP 连接管理：connect / disconnect / reconnect 命令的公共逻辑。
//!
//! 服务器地址来自全局配置（`setg SERVER <addr>` / `setg PORT <port>`，
//! PORT 缺省 50051）。传输协议为自定义帧：
//! u32 大端长度前缀 + prost 编码消息（不用 tonic）。

use crate::LIB;
use anyhow::{Context, Result, anyhow};
use prost::Message;
use safer_ffi::option::TaggedOption;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::sync::{LazyLock, Mutex};

/// 当前活动连接（`None` = 未连接）。
pub static CONNECTION: LazyLock<Mutex<Option<TcpStream>>> =
    LazyLock::new(|| Mutex::new(None));

/// 响应帧队列：后台读线程把非推送的 PACKET 帧入队，`send_request` 取出匹配。
pub static RESP_QUEUE: LazyLock<Mutex<VecDeque<protos::models::packet::Packet>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

const DEFAULT_PORT: &str = "50051";

/// 帧消息类型（帧头第 1 字节；避免空消息被误判为其它消息类型）。
#[allow(dead_code)] // PACKET 暂未使用，供后续 send_packet 通路使用。
pub mod frame_type {
    /// 数据包（models.packet.Packet）。
    pub const PACKET: u8 = 1;
    /// 握手请求（connect.ConnectRequest）。
    pub const CONNECT_REQUEST: u8 = 2;
    /// 握手响应（connect.ConnectResponse）。
    pub const CONNECT_RESPONSE: u8 = 3;
    /// 断开请求（connection.session.DisconnectRequest）。
    pub const DISCONNECT_REQUEST: u8 = 4;
    /// 断开响应（connection.session.DisconnectResponse）。
    pub const DISCONNECT_RESPONSE: u8 = 5;
}

/// 读取一个帧：1 字节 type + u32 大端长度 + 内容。
pub fn read_frame(stream: &mut TcpStream) -> Result<(u8, Vec<u8>)> {
    let mut type_buf = [0u8; 1];
    stream.read_exact(&mut type_buf)?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(anyhow!("frame too large: {len}"));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok((type_buf[0], buf))
}

/// 写入一个帧。
pub fn write_frame(stream: &mut TcpStream, frame_type: u8, bytes: &[u8]) -> Result<()> {
    stream.write_all(&[frame_type])?;
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}

/// 读取 SERVER / PORT 全局配置（PORT 缺省 50051）。
fn server_config() -> Result<(String, String)> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, get_global_option);

    let server = match get_global_option(&"SERVER".into()) {
        TaggedOption::Some(v) => v.to_string(),
        TaggedOption::None => {
            return Err(anyhow!("SERVER not set; use `setg SERVER <address>` first"));
        }
    };
    let port = match get_global_option(&"PORT".into()) {
        TaggedOption::Some(v) => v.to_string(),
        TaggedOption::None => DEFAULT_PORT.to_string(),
    };
    Ok((server, port))
}

/// 建立到服务器的连接并完成握手。
pub fn connect_now() -> Result<()> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, print_content);

    if CONNECTION.lock().unwrap().is_some() {
        print_content(&"Already connected.".into());
        return Ok(());
    }

    let (server, port) = server_config()?;

    // 必须先用 `setg USERNAME` 设置用户名，否则不允许加入。
    let username = require_username()?;

    let addr = format!("{server}:{port}");
    let mut stream =
        TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {addr}"))?;

    // 握手：ConnectRequest → ConnectResponse，携带用户名。
    let mut sync_keys = HashMap::new();
    sync_keys.insert("username".to_string(), username);
    let req = protos::connect::ConnectRequest {
        sync_keys,
        system_info: Some(protos::models::system::SystemInfo {
            name: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            bits: (std::mem::size_of::<usize>() * 8).to_string(),
            vers: "unknown".to_string(),
        }),
        prog_vers: env!("CARGO_PKG_VERSION").to_string(),
    };
    write_frame(&mut stream, frame_type::CONNECT_REQUEST, &req.encode_to_vec())?;

    let (frame_type, resp_buf) = read_frame(&mut stream)?;
    if frame_type != frame_type::CONNECT_RESPONSE {
        return Err(anyhow!("unexpected frame type {frame_type} during handshake"));
    }
    let resp = protos::connect::ConnectResponse::decode(&resp_buf[..])
        .context("Failed to decode ConnectResponse")?;
    let status_str = match resp.status {
        1 => "success",
        2 => "version not compatible",
        100 => "error",
        _ => "unknown",
    };
    print_content(
        &format!("Connect: {status_str} {}.", resp.message.unwrap_or_default()).into(),
    );
    if resp.status != 1 {
        return Err(anyhow!("server rejected the connection"));
    }

    // 常驻后台读线程：持续读帧，推送帧旁路收集，其余响应入队供 send_request 取用。
    let mut reader_stream = stream.try_clone().context("Failed to clone stream for reader")?;
    thread::spawn(move || loop {
        match read_frame(&mut reader_stream) {
            Ok((ft, buf)) => {
                if ft != frame_type::PACKET {
                    // 非 PACKET 帧（如 DisconnectResponse）由主流程处理/忽略。
                    continue;
                }
                if let Ok(p) = protos::models::packet::Packet::decode(&buf[..]) {
                    if p.channel == "notification_push" {
                        collect_push(&p);
                    } else {
                        RESP_QUEUE.lock().unwrap().push_back(p);
                    }
                }
            }
            Err(e) => {
                eprintln!("[cli-standard] reader error: {e}");
                break;
            }
        }
    });

    *CONNECTION.lock().unwrap() = Some(stream);
    print_content(&format!("Connected to {addr}.").into());
    Ok(())
}

/// 断开当前连接：发送 DisconnectRequest，收到响应后关闭。
pub fn disconnect_now() -> Result<()> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, print_content);

    let mut guard = CONNECTION.lock().unwrap();
    match guard.take() {
        Some(mut stream) => {
            let req = protos::connection::session::DisconnectRequest {};
            let _ = write_frame(&mut stream, frame_type::DISCONNECT_REQUEST, &req.encode_to_vec());
            // 读线程收到 DisconnectResponse 后读到 EOF 自然退出；
            // 这里稍等让响应帧有机会到达，再关闭连接。
            std::thread::sleep(std::time::Duration::from_millis(100));
            print_content(&"Disconnected.".into());
            Ok(())
        }
        None => {
            print_content(&"Not connected.".into());
            Ok(())
        }
    }
}

/// 递增序号，用于请求-响应关联。
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 读取全局 `USERNAME`；未设置时返回错误（不允许加入）。
fn require_username() -> Result<String> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, get_global_option);
    match get_global_option(&"USERNAME".into()) {
        TaggedOption::Some(v) => {
            let name = v.to_string();
            if name.trim().is_empty() {
                Err(anyhow!("USERNAME is empty; use `setg USERNAME <name>` first"))
            } else {
                Ok(name)
            }
        }
        TaggedOption::None => {
            Err(anyhow!("USERNAME not set; use `setg USERNAME <name>` first"))
        }
    }
}

/// 通过 Packet 信封向服务器发送一个 RPC 请求并等待响应。
///
/// - `channel`：路由键（如 `send_message`、`create_freq`）；
/// - `payload`：请求 proto 的编码字节；
/// - 返回：响应 proto 的编码字节。
pub fn send_request(channel: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let mut guard = CONNECTION.lock().unwrap();
    let stream = guard.as_mut().ok_or_else(|| anyhow!("Not connected. Use `connect` first."))?;
    // 未设置 USERNAME 不允许参与任何云端操作（加入约束）。
    let user = require_username()?;

    let req = protos::models::packet::Packet {
        channel: channel.to_string(),
        sender: user,
        payload: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, payload),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        seq: SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    };
    write_frame(stream, frame_type::PACKET, &req.encode_to_vec())?;
    drop(guard); // 读由后台线程负责，释放连接锁避免与读线程无关的等待。

    // 等待匹配 seq 的响应帧（推送帧已由读线程旁路）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if std::time::Instant::now() > deadline {
            return Err(anyhow!("timeout waiting for response to `{channel}`"));
        }
        let resp = {
            let mut q = RESP_QUEUE.lock().unwrap();
            q.pop_front()
        };
        if let Some(resp) = resp {
            if resp.seq == req.seq {
                if !resp.payload.is_empty() {
                    let bytes = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        &resp.payload,
                    )
                    .context("Failed to base64-decode response payload")?;
                    return Ok(bytes);
                }
                return Ok(Vec::new());
            }
            // seq 不匹配（理论顺序队列不应出现）：丢弃继续等。
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// 解析并暂存一条服务端推送帧（notification_push 信封 → NotificationMessage）。
fn collect_push(frame: &protos::models::packet::Packet) {
    let Ok(bytes) = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &frame.payload,
    ) else {
        return;
    };
    let Ok(msg) = protos::frequency::notification::NotificationMessage::decode(&bytes[..]) else {
        return;
    };
    // 服务端通知格式为 `[kind:freq] ...`，这里保留 kind、去掉 `0.5] ` 前缀。
    let text = msg
        .text
        .split_once(']')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(msg.text.as_str());
    let display = format!("!! [{}] {}", msg.kind, text);
    // 写入宿主单实例队列（经 LIB 调 cli_core 的 tc_cli_push_enqueue）。
    if let Some(lib) = LIB.get() {
        type EnqueueFn = extern "C" fn(&safer_ffi::String);
        if let Ok(enqueue) = unsafe { lib.get::<EnqueueFn>(b"tc_cli_push_enqueue") } {
            let text: safer_ffi::String = display.into();
            enqueue(&text);
        }
    }
}

/// 取走并清空推送队列（宿主单实例），由 cli 主程序后台线程经 FFI 调用。
pub fn take_pushes() -> Vec<String> {
    let Some(lib) = LIB.get() else { return Vec::new() };
    type DrainFn = unsafe extern "C" fn() -> safer_ffi::Vec<safer_ffi::String>;
    match unsafe { lib.get::<DrainFn>(b"tc_cli_push_drain") } {
        Ok(drain) => {
            let items: Vec<safer_ffi::String> = unsafe { drain() }.into();
            items.into_iter().map(|s| s.to_string()).collect()
        }
        Err(_) => Vec::new(),
    }
}

/// 发送 RPC 请求并解码响应（类型安全的便捷封装）。
pub fn rpc<T: prost::Message + Default, R: prost::Message + Default>(
    channel: &str,
    req: &T,
) -> Result<R> {
    let bytes = send_request(channel, &req.encode_to_vec())?;
    if bytes.is_empty() {
        return Err(anyhow!("server returned an empty response for `{channel}`"));
    }
    R::decode(&bytes[..]).context(format!("Failed to decode response for `{channel}`"))
}

