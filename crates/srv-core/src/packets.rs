//! 数据包接收 / 分发 / 下发。
//!
//! `on_recieve_packet` 是服务端接收数据包的入口（对应客户端侧的
//! 概念，命名沿用 `recieve` 拼写以保持一致）。它按 `packet.channel`
//! 路由到已注册的 channel handler；`send_packet` 是服务端向客户端
//! 下发包的骨架（传输层待接入）。

use crate::channels::{channel_registered, find_channel_handler};
use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use srv_core_types::packets::{Packet, PacketResult};

/// 接收一个数据包并按频道分发。
///
/// - 频道已注册且绑定了 handler → 调用 handler；
/// - 频道已注册但未绑定 handler → 返回 `code=2`；
/// - 频道未注册 → 返回 `code=1`。
///
/// 该函数由 `#[register_hook]` 包裹，插件可通过
/// `register_before_on_recieve_packet` / `register_on_on_recieve_packet` /
/// `register_after_on_recieve_packet` 挂接扩展逻辑。
#[register_hook]
pub fn on_recieve_packet(packet: &Packet) -> PacketResult {
    match find_channel_handler(&packet.channel.to_string()) {
        Some(handler) => handler(packet),
        None => {
            if channel_registered(&packet.channel.to_string()) {
                PacketResult {
                    handled: 0,
                    code: 2,
                    message: format!(
                        "channel '{}' registered but has no handler",
                        packet.channel
                    )
                    .into(),
                }
            } else {
                PacketResult {
                    handled: 0,
                    code: 1,
                    message: format!("unknown channel '{}'", packet.channel).into(),
                }
            }
        }
    }
}

/// 服务端向客户端下发数据包。
///
/// 当前为骨架：仅打印日志。后续接入真实传输层（TCP/WebSocket 等）
/// 后，按 `packet.channel` 找到频道成员并下发。
#[register_hook]
pub fn send_packet(packet: &Packet) -> PacketResult {
    // TODO(transport): 接入真实传输层后按 channel 成员下发。
    println!(
        "[srv-core] send_packet channel={} sender={} seq={} payload={}",
        packet.channel, packet.sender, packet.seq, packet.payload
    );
    PacketResult::handled_ok()
}
