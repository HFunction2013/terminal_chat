//! 会话级配置项（session options）存储。
//!
//! 与 `global_settings.rs`（全局配置）对称：全局配置跨会话共享，
//! 会话配置只在当前会话生效，`set` / `get` / `unset` 命令操作这里。

use ::safer_ffi::prelude::*;
use cli_core_types::{GlobalOption, Result};
use hook_macro::register_hook;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 设置一个会话配置项。
#[register_hook]
fn set_session_option(option: &GlobalOption, out: Out<'_, GlobalOption>) -> Result {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.to_string(), option.value.to_string());
    out.write(option.clone());
    Result::success()
}

/// 会话配置项是否存在。
#[register_hook]
fn exists_session_option(key: &safer_ffi::String) -> Result {
    let map = MAP.lock().unwrap();
    if map.contains_key(&**key) { Result::success() } else { Result::error("not found") }
}

/// 取会话配置项的值。
#[register_hook]
fn get_session_option(
    key: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::String>>,
) -> Result {
    let map = MAP.lock().unwrap();
    let value = map.get(&**key).cloned().map(|v| v.into()).into();
    out.write(value);
    Result::success()
}

/// 删除会话配置项。
#[register_hook]
fn remove_session_option(
    key: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::String>>,
) -> Result {
    let mut map = MAP.lock().unwrap();
    let old = map.remove(&**key).map(|v| v.into()).into();
    out.write(old);
    Result::success()
}

/// 取全部会话配置项。
#[register_hook]
fn get_all_session_options(out: Out<'_, safer_ffi::vec::Vec<GlobalOption>>) -> Result {
    let map = MAP.lock().unwrap();
    let vec: safer_ffi::vec::Vec<GlobalOption> =
        map.iter().map(|(k, v)| GlobalOption::new(k, v)).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 清空所有会话配置项。
#[register_hook]
fn clear_all_session_options() -> Result {
    let mut map = MAP.lock().unwrap();
    map.clear();
    Result::success()
}
