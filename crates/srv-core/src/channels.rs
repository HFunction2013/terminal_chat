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
use srv_core_types::Result;
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
pub fn is_channel_registered(identifier: &safer_ffi::String) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    if map.contains_key(&identifier.to_string()) {
        Result::success()
    } else {
        Result::error("not found")
    }
}

/// 注册一个频道（name 为唯一键；已存在则失败）。
#[register_hook]
pub fn register_channel(metadata: &ChannelMetadata) -> Result {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let key = metadata.name.to_string();
    if map.contains_key(&key) {
        Result::error("already registered")
    } else {
        map.insert(key, metadata.clone());
        Result::success()
    }
}

/// 注销一个频道（同时移除其 handler）。
#[register_hook]
pub fn unregister_channel(name: &safer_ffi::String) -> Result {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    if map.remove(&name.to_string()).is_some() {
        handlers.remove(&name.to_string());
        Result::success()
    } else {
        Result::error("not found")
    }
}

/// 通过名称获取频道元数据。
#[register_hook]
pub fn get_channel(
    name: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::boxed::ThinBox<ChannelMetadata>>>,
) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    let value = match map.get(&name.to_string()) {
        Some(c) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(c.clone())),
        None => TaggedOption::None,
    };
    out.write(value);
    Result::success()
}

/// 检查频道是否存在。
#[register_hook]
pub fn has_channel(name: &safer_ffi::String) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    if map.contains_key(&name.to_string()) { Result::success() } else { Result::error("not found") }
}

/// 获取频道数量（经出参回填；状态经 Result.code 表达）。
#[register_hook]
pub fn channel_count(out: Out<'_, i32>) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    out.write(map.len() as i32);
    Result::success()
}

/// 清空所有频道。
#[register_hook]
pub fn clear_channels() -> Result {
    let mut map = CHANNEL_MAP.lock().unwrap();
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    map.clear();
    handlers.clear();
    Result::success()
}

/// 获取全部频道元数据。
#[register_hook]
pub fn get_all_channels(out: Out<'_, safer_ffi::vec::Vec<ChannelMetadata>>) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    let channels: Vec<ChannelMetadata> = map.values().cloned().collect();
    out.write(channels.into());
    Result::success()
}

/// 绑定频道的处理函数（FFI 导出，两参数故不用 `register_hook`）。
///
/// `handler` 必须是插件导出的 `extern "C" fn(*const Packet) -> PacketResult`
/// 函数指针。成功返回 `Result::success()`，频道不存在或重复绑定返回错误。
#[ffi_export]
pub fn set_channel_handler(channel_name: &safer_ffi::String, handler: ChannelHandlerFn) -> Result {
    let map = CHANNEL_MAP.lock().unwrap();
    if !map.contains_key(&channel_name.to_string()) {
        // 频道未注册，拒绝绑定。
        return Result::error("channel not found");
    }
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    handlers.insert(channel_name.to_string(), handler);
    Result::success()
}

/// 移除频道的处理函数。
#[ffi_export]
pub fn clear_channel_handler(channel_name: &safer_ffi::String) -> Result {
    let mut handlers = CHANNEL_HANDLERS.lock().unwrap();
    if handlers.remove(&channel_name.to_string()).is_some() {
        Result::success()
    } else {
        Result::error("not found")
    }
}

/// 频道是否绑定了处理函数。
#[ffi_export]
pub fn has_channel_handler(channel_name: &safer_ffi::String) -> Result {
    let handlers = CHANNEL_HANDLERS.lock().unwrap();
    if handlers.contains_key(&channel_name.to_string()) {
        Result::success()
    } else {
        Result::error("not found")
    }
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
