//! 会话级配置项（session options）存储。
//!
//! 与 `global_settings.rs`（全局配置）对称：全局配置跨会话共享，
//! 会话配置只在当前会话生效，`set` / `get` / `unset` 命令操作这里。

use ::safer_ffi::prelude::*;
use cli_core_types::GlobalOption;
use hook_macro::register_hook;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 设置一个会话配置项。
#[register_hook]
fn set_session_option(option: &GlobalOption) -> GlobalOption {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.to_string(), option.value.to_string());
    option.clone()
}

/// 会话配置项是否存在。
#[register_hook]
fn exists_session_option(key: &safer_ffi::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**key)
}

/// 取会话配置项的值。
#[register_hook(fallback = "TaggedOption::None")]
fn get_session_option(key: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let map = MAP.lock().unwrap();
    map.get(&**key).cloned().map(|v| v.into()).into()
}

/// 删除会话配置项。
#[register_hook(fallback = "TaggedOption::None")]
fn remove_session_option(key: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**key).map(|v| v.into()).into()
}

/// 取全部会话配置项。
#[register_hook(fallback = "safer_ffi::vec::Vec::from(Vec::new())")]
fn get_all_session_options() -> safer_ffi::vec::Vec<GlobalOption> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| GlobalOption::new(k, v)).collect::<Vec<_>>().into()
}

/// 清空所有会话配置项。
#[register_hook]
fn clear_all_session_options() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}
