//! srv：terminal_chat 服务端（二进制）。
//!
//! 启动流程：
//! 1. 用 `which_dylib` 定位并加载 `srv_core`（libsrv_core.so）；
//! 2. 通过 srv-core 的 `load_plugin` 默认加载 `srv_standard`
//!    （libsrv_standard.so），由它注册通话/聊天等频道及 handler；
//! 3. 启动 TCP 监听（`--port` / `SRV_PORT` / 默认 50051），
//!    为每个客户端连接建立会话，处理握手、数据包分发与断开。
//!
//! 传输协议（自定义帧，prost 编码，不用 tonic）：
//! - 帧：u32 大端长度前缀 + prost 编码的消息字节；
//! - 握手：客户端发 `ConnectRequest` → 服务端回 `ConnectResponse`；
//! - 数据：双方互发 `models.packet.Packet`，服务端按 channel
//!   交给 srv-core 的 `on_recieve_packet` 分发；
//! - 断开：客户端发 `DisconnectRequest` → 服务端回
//!   `DisconnectResponse` 后关闭连接并注销会话。

use anyhow::{Result, anyhow};
use libloading::Library;
use prost::Message;
use srv_core_types::sessions::SessionInfo;
use srv_core_types::{
    AddSessionFn, LoadPluginFn, OnRecievePacketFn, RemoveSessionFn, SessionCountFn,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use which_dylib::{FindError, FindLibBuilder};

static SRV_CORE: OnceLock<Library> = OnceLock::new();

fn get_srv_core() -> &'static Library {
    match FindLibBuilder::new().find_result("srv_core") {
        Ok(s) => {
            let lib_path = s.to_str().expect("Failed to convert `PathBuf` to `&str`");

            SRV_CORE.get_or_init(|| unsafe {
                Library::new(lib_path)
                    .unwrap_or_else(|e| panic!("Failed to load library '{}': {}", lib_path, e))
            })
        }
        Err(e) => match e {
            FindError::NotFound(s) => panic!("NotFoundError: {}", s),
            FindError::Ambiguous(v) => panic!("AmbiguousError: {:?}", v),
        },
    }
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 帧消息类型（帧头第 1 字节；空消息（如 DisconnectRequest）也能明确区分）。
mod frame_type {
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
fn read_frame(stream: &mut TcpStream) -> Result<(u8, Vec<u8>)> {
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
fn write_frame(stream: &mut TcpStream, frame_type: u8, bytes: &[u8]) -> Result<()> {
    stream.write_all(&[frame_type])?;
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}

/// 处理单个客户端连接：握手 → 会话注册 → 帧循环 → 断开清理。
fn handle_client(mut stream: TcpStream, symbols: SrvSymbols) {
    let peer = match stream.peer_addr() {
        Ok(a) => a.to_string(),
        Err(_) => "unknown".to_string(),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(300)));

    // 握手：读 ConnectRequest → 回 ConnectResponse。
    let handshake_ok = match read_frame(&mut stream) {
        Ok((frame_type, buf)) if frame_type == frame_type::CONNECT_REQUEST => {
            match protos::connect::ConnectRequest::decode(&buf[..]) {
                Ok(req) => {
                    println!("[srv] handshake from {peer}: vers={}", req.prog_vers);
                    let resp = protos::connect::ConnectResponse {
                        status: protos::connect::ConnectStatus::ConnectSuccess as i32,
                        message: Some(format!("welcome, {}", peer)),
                    };
                    let _ = write_frame(&mut stream, frame_type::CONNECT_RESPONSE, &resp.encode_to_vec());
                    true
                }
                Err(e) => {
                    println!("[srv] bad ConnectRequest from {peer}: {e}");
                    false
                }
            }
        }
        Ok((ft, _)) => {
            println!("[srv] unexpected handshake frame type {ft} from {peer}");
            false
        }
        Err(e) => {
            println!("[srv] handshake read failed from {peer}: {e}");
            false
        }
    };
    if !handshake_ok {
        return;
    }

    // 注册会话。
    let session = SessionInfo {
        id: peer.clone().into(),
        peer_addr: peer.clone().into(),
        connected_at: now_ts(),
        channel: "".into(),
        state: 0,
    };
    (symbols.add_session)(&session);
    println!("[srv] session registered: {peer} (total={})", (symbols.session_count)());

    // 帧循环：Packet 分发 / DisconnectRequest 断开 / EOF 清理。
    loop {
        let (frame_type, buf) = match read_frame(&mut stream) {
            Ok(f) => f,
            Err(_) => break, // EOF 或超时 → 视为断开
        };
        match frame_type {
            frame_type::PACKET => {
                if let Ok(packet) = protos::models::packet::Packet::decode(&buf[..]) {
                    let sp = srv_core_types::packets::Packet {
                        channel: packet.channel.clone().into(),
                        sender: packet.sender.clone().into(),
                        payload: packet.payload.into(),
                        timestamp: packet.timestamp,
                        seq: packet.seq,
                    };
                    let res = (symbols.on_recieve_packet)(&sp);
                    println!(
                        "[srv] packet from {peer}: channel={} handled={} code={} msg={}",
                        sp.channel, res.handled, res.code, res.message
                    );
                    // 若 handler 携带了响应负载（base64），构造响应 Packet 写回客户端。
                    if !res.payload.is_empty() {
                        let resp_packet = protos::models::packet::Packet {
                            channel: sp.channel.clone().into(),
                            sender: "srv".to_string(),
                            payload: res.payload.clone().into(),
                            timestamp: now_ts(),
                            seq: sp.seq,
                        };
                        if let Err(e) = write_frame(
                            &mut stream,
                            frame_type::PACKET,
                            &resp_packet.encode_to_vec(),
                        ) {
                            println!("[srv] failed to write response to {peer}: {e}");
                            break;
                        }
                    }
                }
            }
            frame_type::DISCONNECT_REQUEST => {
                let resp = protos::connection::session::DisconnectResponse {
                    result: protos::connection::session::DisconnectResult::DisconnectSuccess as i32,
                    message: "bye".to_string(),
                };
                let _ = write_frame(&mut stream, frame_type::DISCONNECT_RESPONSE, &resp.encode_to_vec());
                println!("[srv] disconnect requested by {peer}");
                break;
            }
            other => {
                println!("[srv] unknown frame type {other} from {peer}, ignoring");
            }
        }
    }

    // 清理会话。
    let removed = (symbols.remove_session)(&peer.clone().into());
    println!("[srv] session removed: {peer} removed={removed}");
}

/// 线程内需要的 srv-core fn 指针（fn 指针是 Copy+Send，可直接跨线程）。
#[derive(Clone, Copy)]
struct SrvSymbols {
    on_recieve_packet: OnRecievePacketFn,
    add_session: AddSessionFn,
    remove_session: RemoveSessionFn,
    session_count: SessionCountFn,
}

fn main() -> Result<()> {
    // 获取 srv-core 符号（fn 指针，便于跨线程调用）。
    let load_plugin: LoadPluginFn = *unsafe { get_srv_core().get(b"load_plugin") }
        .map_err(|e| anyhow!("Failed to find symbol 'load_plugin': {e}"))?;
    let on_recieve_packet: OnRecievePacketFn = *unsafe { get_srv_core().get(b"on_recieve_packet") }
        .map_err(|e| anyhow!("Failed to find symbol 'on_recieve_packet': {e}"))?;
    let add_session: AddSessionFn = *unsafe { get_srv_core().get(b"add_session") }
        .map_err(|e| anyhow!("Failed to find symbol 'add_session': {e}"))?;
    let remove_session: RemoveSessionFn = *unsafe { get_srv_core().get(b"remove_session") }
        .map_err(|e| anyhow!("Failed to find symbol 'remove_session': {e}"))?;
    let session_count: SessionCountFn = *unsafe { get_srv_core().get(b"session_count") }
        .map_err(|e| anyhow!("Failed to find symbol 'session_count': {e}"))?;

    // 默认加载 srv-standard。
    let res = load_plugin(&"srv_standard".into());
    if res.success != 1 {
        eprintln!(
            "Failed to load srv_standard: exit_code={} msg={:?}",
            res.exit_code, res.msg
        );
        std::process::exit(1);
    }
    println!("[srv] srv-standard loaded.");

    // 端口：--port <N> 参数 > SRV_PORT 环境变量 > 默认 50051。
    let port = std::env::args()
        .position(|a| a == "--port")
        .and_then(|i| std::env::args().nth(i + 1))
        .or_else(|| std::env::var("SRV_PORT").ok())
        .unwrap_or_else(|| "50051".to_string());
    let addr = format!("[::]:{port}");
    let listener = TcpListener::bind(&addr)?;
    println!("[srv] listening on {addr} (TCP, prost frames)");

    let symbols = SrvSymbols { on_recieve_packet, add_session, remove_session, session_count };

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let symbols = symbols;
                thread::spawn(move || handle_client(stream, symbols));
            }
            Err(e) => {
                println!("[srv] accept error: {e}");
            }
        }
    }
    Ok(())
}
