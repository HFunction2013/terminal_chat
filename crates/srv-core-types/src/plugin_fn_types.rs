//! srv-core 导出的插件约定函数类型（手动声明部分）。
//!
//! 注意：`#[register_hook]` 标记的函数类型由 build.rs 自动生成并
//! `include!` 进 lib.rs（如 `RegisterChannelFn`、`OnRecievePacketFn`、
//! `LoadPluginFn` 等）。这里只手动声明 build.rs 扫描不到的、由
//! `#[ffi_export]` 导出的函数类型。

use crate::packets::Packet;
use crate::packets::PacketResult;

/// 频道处理函数：srv-core 收到包后按频道路由，调用该指针。
///
/// 由插件（如 srv-standard）以 `extern "C" fn` 提供，
/// 通过 srv-core 的 `set_channel_handler` 注册。
pub type ChannelHandlerFn = extern "C" fn(*const Packet) -> PacketResult;

/// srv-core 的 `set_channel_handler` 函数指针类型。
///
/// 签名：`set_channel_handler(channel_name, handler) -> i32`。
pub type SetChannelHandlerFn = extern "C" fn(&safer_ffi::String, ChannelHandlerFn) -> i32;
