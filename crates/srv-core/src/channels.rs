//! 频道（channel）注册与管理。
//!
//! 频道是服务端路由数据包的基本单位。插件在 `on_init_plugin` 中
//! 用 `register_channel` 登记频道元数据，再用 `set_channel_handler`
//! 绑定该频道的处理函数；`on_recieve_packet` 收到包后按频道名
//! 找到 handler 并调用。
//!
//! 通话（call）频道、聊天（chat）频道等只是 `ChannelMetadata` 中
//! `channel_type` 的不同取值，注册机制完全一致。

use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};

use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use srv_core_types::channels::ChannelMetadata;
use srv_core_types::plugin_fn_types::ChannelHandlerFn;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// 频道元数据表：频道名 → 元数据。
static CHANNEL_MAP: LazyLock<Mutex<HashMap<String, ChannelMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 频道处理函数表：频道名 → 处理函数指针。
static CHANNEL_HANDLERS: LazyLock<Mutex<HashMap<String, ChannelHandlerFn>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 频道名是否已注册。
#[register_hook]
pub fn is_channel_registered(identifier: &safer_ffi::String) -> bool {
    let map = CHANNEL_MAP.lock().unwrap();
    map.contains_key(&identifier.to_string())
}

/// 注册一个频道（name 为唯一键；已存在则失败）。
#[register_hook]
pub fn register_channel(metadata: &ChannelMetadata) -> i32 {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let key = metadata.name.to_string();
    if map.contains_key(&key) {
        false.into()
    } else {
        map.insert(key, metadata.clone());
        true.into()
    }
}

/// 注销一个频道（同时移除其 handler）。
#[register_hook]
pub fn unregister_channel(name: &safer_ffi::String) -> i32 {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    if map.remove(&name.to_string()).is_some() {
        handlers.remove(&name.to_string());
        true.into()
    } else {
        false.into()
    }
}

/// 通过名称获取频道元数据。
#[register_hook(fallback = "TaggedOption::None")]
pub fn get_channel(
    name: &safer_ffi::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<ChannelMetadata>> {
    let map = CHANNEL_MAP.lock().unwrap();
    match map.get(&name.to_string()) {
        Some(c) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(c.clone())),
        None => TaggedOption::None,
    }
}

/// 检查频道是否存在。
#[register_hook]
pub fn has_channel(name: &safer_ffi::String) -> bool {
    let map = CHANNEL_MAP.lock().unwrap();
    map.contains_key(&name.to_string())
}

/// 获取频道数量。
#[register_hook]
pub fn channel_count() -> i32 {
    let map = CHANNEL_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有频道。
#[register_hook]
pub fn clear_channels() {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    map.clear();
    handlers.clear();
}

/// 获取全部频道元数据。
#[register_hook(fallback = "safer_ffi::Vec::from(std::vec::Vec::new())")]
pub fn get_all_channels() -> safer_ffi::Vec<ChannelMetadata> {
    let map = CHANNEL_MAP.lock().unwrap();
    let channels: Vec<ChannelMetadata> = map.values().cloned().collect();
    channels.into()
}

/// 绑定频道的处理函数（FFI 导出，两参数故不用 `register_hook`）。
///
/// `handler` 必须是插件导出的 `extern "C" fn(*const Packet) -> PacketResult`
/// 函数指针。成功返回 `1`，频道不存在或重复绑定返回 `0`。
#[ffi_export]
pub fn set_channel_handler(channel_name: &safer_ffi::String, handler: ChannelHandlerFn) -> i32 {
    let map = CHANNEL_MAP.lock().unwrap();
    if !map.contains_key(&channel_name.to_string()) {
        // 频道未注册，拒绝绑定。
        return false.into();
    }
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    handlers.insert(channel_name.to_string(), handler);
    true.into()
}

/// 移除频道的处理函数。
#[ffi_export]
pub fn clear_channel_handler(channel_name: &safer_ffi::String) -> i32 {
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    handlers.remove(&channel_name.to_string()).map(|_| true).unwrap_or(false).into()
}

/// 频道是否绑定了处理函数。
#[ffi_export]
pub fn has_channel_handler(channel_name: &safer_ffi::String) -> bool {
    let handlers = CHANNEL_HANDLERS.lock().unwrap();
    handlers.contains_key(&channel_name.to_string())
}

/// （内部）按频道名查找处理函数，供包分发使用。
pub(crate) fn find_channel_handler(name: &str) -> Option<ChannelHandlerFn> {
    let handlers = CHANNEL_HANDLERS.lock().unwrap();
    handlers.get(name).copied()
}

/// （内部）频道是否已注册，供包分发使用。
pub(crate) fn channel_registered(name: &str) -> bool {
    let map = CHANNEL_MAP.lock().unwrap();
    map.contains_key(name)
}
