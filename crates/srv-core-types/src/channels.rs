//! 服务端频道（channel）类型。
//!
//! 频道是服务端消息路由的基本单位：客户端把包发给某个频道，
//! srv-core 的 `on_recieve_packet` 按 `packet.channel` 找到对应的
//! channel handler 并调用。通话（call）频道、聊天（chat）频道、
//! 文件（file）频道等都是具体频道类型。

use safer_ffi::prelude::*;

/// 频道类型常量（以 i32 存储以保持 FFI 布局稳定，避免枚举布局差异）。
pub mod channel_type {
    /// 普通聊天频道。
    pub const CHAT: i32 = 0;
    /// 通话频道（语音/视频通话）。
    pub const CALL: i32 = 1;
    /// 文件传输频道。
    pub const FILE: i32 = 2;
    /// 广播频道。
    pub const BROADCAST: i32 = 3;
    /// 自定义/插件频道。
    pub const CUSTOM: i32 = 4;
}

/// 一个已注册频道的元数据。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ChannelMetadata {
    /// 频道名（路由键，如 `"call"`、`"chat"`）。
    pub name: safer_ffi::String,
    /// 频道类型，取值见 [`channel_type`]。
    pub channel_type: i32,
    /// 频道用途描述。
    pub description: safer_ffi::String,
    /// 注册该频道的插件版本。
    pub version: safer_ffi::String,
    /// 注册该频道的插件作者。
    pub author: safer_ffi::String,
    /// 频道容量（最大并发成员数，`-1` 表示无限制）。
    pub capacity: i32,
}

impl Default for ChannelMetadata {
    fn default() -> Self {
        Self {
            name: "".into(),
            channel_type: channel_type::CUSTOM,
            description: "".into(),
            version: "".into(),
            author: "".into(),
            capacity: -1,
        }
    }
}
