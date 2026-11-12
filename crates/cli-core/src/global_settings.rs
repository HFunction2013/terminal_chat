use ::safer_ffi::prelude::*;
use cli_core_types::{GlobalOption, Result};
use hook_macro::register_hook;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 内部直读：`QUIET` 是否开启（供宿主静音判定，不经 FFI 出参包装）。
pub(crate) fn quiet_enabled() -> bool {
    let map = MAP.lock().unwrap();
    map.get("QUIET")
        .map(|v| v.to_lowercase())
        .map(|qv| matches!(qv.as_str(), "on" | "true" | "1" | "yes"))
        .unwrap_or(false)
}

#[register_hook]
fn set_global_option(option: &GlobalOption, out: Out<'_, GlobalOption>) -> Result {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.to_string(), option.value.to_string());
    out.write(option.clone());
    Result::success()
}

#[register_hook]
fn exists_global_option(key: &safer_ffi::String) -> Result {
    let map = MAP.lock().unwrap();
    if map.contains_key(&**key) { Result::success() } else { Result::error("not found") }
}

#[register_hook]
fn get_global_option(
    key: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::String>>,
) -> Result {
    let map = MAP.lock().unwrap();
    let value = map.get(&**key).cloned().map(|v| v.into()).into();
    out.write(value);
    Result::success()
}

#[register_hook]
fn remove_global_option(
    key: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::String>>,
) -> Result {
    let mut map = MAP.lock().unwrap();
    let old = map.remove(&**key).map(|v| v.into()).into();
    out.write(old);
    Result::success()
}

#[register_hook]
fn get_all_options(out: Out<'_, safer_ffi::vec::Vec<GlobalOption>>) -> Result {
    let map = MAP.lock().unwrap();
    let vec: safer_ffi::vec::Vec<GlobalOption> =
        map.iter().map(|(k, v)| GlobalOption::new(k, v)).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

#[register_hook]
fn clear_all_options() -> Result {
    let mut map = MAP.lock().unwrap();
    map.clear();
    Result::success()
}
