//! TCP 连接管理：connect / disconnect / reconnect 命令的公共逻辑。
//!
//! 服务器地址来自全局配置（`setg SERVER <addr>` / `setg PORT <port>`，
//! PORT 缺省 50051）。传输协议为自定义帧：
//! u32 大端长度前缀 + prost 编码消息（不用 tonic）。

use crate::LIB;
use anyhow::{Context, Result, anyhow};
use prost::Message;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{LazyLock, Mutex};

/// 当前活动连接（`None` = 未连接）。
pub static CONNECTION: LazyLock<Mutex<Option<TcpStream>>> =
    LazyLock::new(|| Mutex::new(None));

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
    let addr = format!("{server}:{port}");
    let mut stream =
        TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {addr}"))?;

    // 握手：ConnectRequest → ConnectResponse。
    let req = protos::connect::ConnectRequest {
        sync_keys: HashMap::new(),
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
            // 尽力读取响应后关闭。
            if let Ok((ft, buf)) = read_frame(&mut stream)
                && ft == frame_type::DISCONNECT_RESPONSE
                && let Ok(resp) =
                    protos::connection::session::DisconnectResponse::decode(&buf[..])
            {
                print_content(&format!("Disconnect: {}.", resp.message).into());
            }
            print_content(&"Disconnected.".into());
            Ok(())
        }
        None => {
            print_content(&"Not connected.".into());
            Ok(())
        }
    }
}
