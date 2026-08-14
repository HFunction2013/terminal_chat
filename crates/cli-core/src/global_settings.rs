use ::safer_ffi::prelude::*;
use hook_macro::register_hook;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use cli_core_types::GlobalOption;

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[register_hook]
fn set_global_option_impl(option: &GlobalOption) -> GlobalOption {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.to_string(), option.value.to_string());
    option.clone()
}

#[register_hook]
fn exists_global_option_impl(key: &safer_ffi::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**key)
}

#[register_hook(fallback = "TaggedOption::None")]
fn get_global_option_impl(key: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let map = MAP.lock().unwrap();
    map.get(&**key).cloned().map(|v| v.into()).into()
}

#[register_hook(fallback = "TaggedOption::None")]
fn remove_global_option_impl(key: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**key).map(|v| v.into()).into()
}

#[register_hook(fallback = "safer_ffi::vec::Vec::from(Vec::new())")]
fn get_all_options_impl() -> safer_ffi::vec::Vec<GlobalOption> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| GlobalOption::new(k, v)).collect::<Vec<_>>().into()
}

#[register_hook]
fn clear_all_options_impl() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}
