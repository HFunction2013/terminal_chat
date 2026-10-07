//! 服务端数据包（packet）类型。
//!
//! `Packet` 是客户端与服务端之间的最小消息单元。srv-core 的
//! `on_recieve_packet` 接收 `Packet`，按 `channel` 字段路由到
//! 对应的 channel handler。

use safer_ffi::prelude::*;

/// 一个待处理的数据包。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Packet {
    /// 目标频道名（路由键）。
    pub channel: safer_ffi::String,
    /// 发送者标识（如会话 id / 用户名）。
    pub sender: safer_ffi::String,
    /// 包负载（文本；二进制后续可换 base64）。
    pub payload: safer_ffi::String,
    /// 发送时间戳（Unix 秒）。
    pub timestamp: u64,
    /// 递增序号，用于乱序检测。
    pub seq: u64,
}

impl Default for Packet {
    fn default() -> Self {
        Self {
            channel: "".into(),
            sender: "".into(),
            payload: "".into(),
            timestamp: 0,
            seq: 0,
        }
    }
}

/// 包处理结果。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PacketResult {
    /// 是否已被某个 handler 处理（`1` = handled）。
    pub handled: i32,
    /// 处理状态码（`0` = ok，其余为错误码）。
    pub code: i32,
    /// 处理说明 / 错误信息。
    pub message: safer_ffi::String,
}

impl Default for PacketResult {
    fn default() -> Self {
        Self { handled: 0, code: 0, message: "".into() }
    }
}

impl PacketResult {
    /// 构造一个“已处理且成功”的结果。
    pub fn handled_ok() -> Self {
        Self { handled: 1, code: 0, message: "ok".into() }
    }

    /// 构造一个“未处理”的结果。
    pub fn unhandled(msg: impl Into<String>) -> Self {
        Self { handled: 0, code: 1, message: msg.into().into() }
    }
}
