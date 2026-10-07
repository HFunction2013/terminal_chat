//! srv-standard：服务端标准插件（cdylib）。
//!
//! 仿照 `cli-standard`，是 srv 启动时默认加载的服务端插件。
//! 它在 `on_init_plugin` 中通过 srv-core 的符号注册默认频道
//! （通话 `call` 频道、聊天 `chat` 频道）并绑定各自的
//! packet handler。当前只提供骨架，不实现具体业务逻辑。

use libloading::{Library, Symbol};
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use srv_core_types::channels::{ChannelMetadata, channel_type};
use srv_core_types::packets::{Packet, PacketResult};
use srv_core_types::plugin_fn_types::{ChannelHandlerFn, SetChannelHandlerFn};
use srv_core_types::plugins::{SrvHostMetadata, SrvPluginMetadata, SrvPluginResult};
use srv_core_types::RegisterChannelFn;
use std::sync::{LazyLock, OnceLock};

static METADATA: LazyLock<SrvPluginMetadata> = LazyLock::new(|| SrvPluginMetadata {
    name: "srv-standard".into(),
    dylib_name: "srv_standard".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "srv standard channels and packet handlers.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::None,
    max_host_version: TaggedOption::None,
});

static LIB: OnceLock<Library> = OnceLock::new();

fn init_library(path: &str) -> &'static Library {
    LIB.get_or_init(|| unsafe { Library::new(path).expect("Failed to load library") })
}

/// 通话频道处理函数：收到 `call` 频道的包时被 srv-core 调用。
extern "C" fn on_call_packet(packet: *const Packet) -> PacketResult {
    let packet = unsafe { &*packet };
    // TODO(call): 通话信令处理（拨号/接听/挂断/媒体协商）待实现。
    println!(
        "[srv-standard] call channel: packet from '{}' seq={}: {}",
        packet.sender, packet.seq, packet.payload
    );
    PacketResult::handled_ok()
}

/// 聊天频道处理函数：收到 `chat` 频道的包时被 srv-core 调用。
extern "C" fn on_chat_packet(packet: *const Packet) -> PacketResult {
    let packet = unsafe { &*packet };
    // TODO(chat): 消息存储与广播待实现。
    println!(
        "[srv-standard] chat channel: packet from '{}' seq={}: {}",
        packet.sender, packet.seq, packet.payload
    );
    PacketResult::handled_ok()
}

/// 构造通话频道元数据。
fn call_channel_metadata() -> ChannelMetadata {
    ChannelMetadata {
        name: "call".into(),
        channel_type: channel_type::CALL,
        description: "Voice/video call channel".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
        capacity: -1,
    }
}

/// 构造聊天频道元数据。
fn chat_channel_metadata() -> ChannelMetadata {
    ChannelMetadata {
        name: "chat".into(),
        channel_type: channel_type::CHAT,
        description: "Public chat channel".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
        capacity: -1,
    }
}

/// 向 srv-core 注册频道并绑定 handler。
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

    register_channel_with_handler(lib, &call_channel_metadata(), on_call_packet);
    register_channel_with_handler(lib, &chat_channel_metadata(), on_chat_packet);

    SrvPluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> SrvPluginResult {
    SrvPluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}
