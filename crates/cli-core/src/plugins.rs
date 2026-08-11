use ::safer_ffi::prelude::*;
use safer_ffi::option::TaggedOption;
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub command_yaml: safer_ffi::String,
    pub name: safer_ffi::String,
    pub command_name: safer_ffi::String,
    pub version: safer_ffi::String,
    pub author: safer_ffi::String,
    pub description: safer_ffi::String,
    pub homepage: safer_ffi::String,
    pub license: safer_ffi::String,
    pub min_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
    pub max_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
    pub pb_len: i32,
}

use safer_ffi::ffi_export;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 注册插件（以 name 为键）
#[ffi_export]
pub fn register_plugin(metadata: &PluginMetadata) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let key = metadata.name.to_string();
    if map.contains_key(&key) {
        false.into() // 已存在，注册失败
    } else {
        map.insert(key, metadata.clone());
        true.into()
    }
}

/// 通过名称获取插件
#[ffi_export]
pub fn get_plugin(name: repr_c::String) -> TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>> {
    let map = PLUGIN_MAP.lock().unwrap();
    match map.get(&name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 删除插件
#[ffi_export]
pub fn unregister_plugin(name: repr_c::String) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    map.remove(&name.to_string()).is_some().into()
}

/// 检查插件是否存在
#[ffi_export]
pub fn has_plugin(name: repr_c::String) -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&name.to_string()).into()
}

/// 获取插件数量
#[ffi_export]
pub fn plugin_count() -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有插件
#[ffi_export]
pub fn clear_plugins() {
    let mut map = PLUGIN_MAP.lock().unwrap();
    map.clear();
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub success: i32,
    pub exit_code: i32,
}
