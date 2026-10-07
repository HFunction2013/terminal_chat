//! 服务端会话（session）类型。
//!
//! 会话代表一个已连接的客户端。当前只提供骨架，
//! 具体连接协议（TCP/WebSocket 等）后续接入。

use safer_ffi::prelude::*;

/// 一个客户端会话的元信息。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 会话 id（连接唯一标识）。
    pub id: safer_ffi::String,
    /// 对端地址（如 `"127.0.0.1:54321"`）。
    pub peer_addr: safer_ffi::String,
    /// 连接建立时间戳（Unix 秒）。
    pub connected_at: u64,
    /// 当前所在频道名（空 = 未加入任何频道）。
    pub channel: safer_ffi::String,
    /// 会话状态（`0` = 在线，`1` = 离线/挂起）。
    pub state: i32,
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self {
            id: "".into(),
            peer_addr: "".into(),
            connected_at: 0,
            channel: "".into(),
            state: 0,
        }
    }
}
