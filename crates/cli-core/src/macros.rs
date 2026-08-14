use ::safer_ffi::option::TaggedOption;
use ::safer_ffi::prelude::*;
use cli_core_types::MacroDef;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[register_hook]
fn set_macro_impl(r#macro: &MacroDef) -> MacroDef {
    let mut map = MAP.lock().unwrap();
    map.insert(r#macro.name.to_string(), r#macro.code.to_string());
    r#macro.clone()
}

#[register_hook]
fn exists_macro_impl(name: &safer_ffi::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**name)
}

#[register_hook(fallback = "TaggedOption::None")]
fn get_macro_impl(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let map = MAP.lock().unwrap();
    map.get(&**name).cloned().map(|v| v.into()).into()
}

#[register_hook(fallback = "TaggedOption::None")]
fn remove_macro_impl(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**name).map(|v| v.into()).into()
}

#[register_hook(fallback = "safer_ffi::vec::Vec::from(Vec::new())")]
fn get_all_macros_impl() -> safer_ffi::vec::Vec<MacroDef> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| MacroDef::new(k, v)).collect::<Vec<_>>().into()
}

#[register_hook]
fn clear_all_macros_impl() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}
