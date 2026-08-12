use ::safer_ffi::prelude::*;
use hook_macro::register_hook;
use libloading::{Library, library_filename};
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

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

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub success: i32,
    pub exit_code: i32,
    pub msg: TaggedOption<safer_ffi::String>,
}

impl Default for PluginResult {
    fn default() -> Self {
        Self { success: 0, exit_code: 0, msg: None.into() }
    }
}

static PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static COMMAND_PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[register_hook]
pub fn is_plugin_name_registered_impl(identifier: &safer_ffi::String) -> bool {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&identifier.to_string())
}

#[register_hook]
pub fn is_plugin_command_name_registered_impl(identifier: &safer_ffi::String) -> bool {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    cmd_map.contains_key(&identifier.to_string())
}

pub(crate) fn get_plugin_lib(plugin_name: &safer_ffi::String) -> Result<libloading::Library, libloading::Error> {
    unsafe { Library::new(library_filename(plugin_name.to_string())) }
}

#[register_hook]
pub fn load_plugin_impl(plugin_path: &repr_c::String) -> PluginResult {
    unsafe {
        let lib = match Library::new(plugin_path.to_string()) {
            Ok(lib) => lib,
            Err(_) => {
                // Err loading.
                return PluginResult {
                    success: 0,
                    exit_code: 101,
                    msg: TaggedOption::Some("Failed to load plugin".into()),
                };
            }
        };

        let get_plugin_metadata =
            match lib.get::<unsafe extern "C" fn() -> PluginMetadata>(b"get_plugin_metadata") {
                Ok(f) => f,
                Err(_) => {
                    // Err loading.
                    return PluginResult {
                        success: 0,
                        exit_code: 102,
                        msg: TaggedOption::Some("Failed to get `get_plugin_metadata`".into()),
                    };
                }
            };
        let meta: PluginMetadata = get_plugin_metadata();
        register_plugin(&meta);
        // TODO: register commands.
    }
    PluginResult { success: 1, exit_code: 0, msg: TaggedOption::None }
}

/// 注册插件（同时以 name 和 command_name 为键）
#[register_hook]
pub fn register_plugin_impl(metadata: &PluginMetadata) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap(); // 新增

    let key = metadata.name.to_string();
    let cmd_key = metadata.command_name.to_string(); // 新增

    if map.contains_key(&key) || cmd_map.contains_key(&cmd_key) {
        // 修改条件
        false.into() // 已存在，注册失败
    } else {
        map.insert(key, metadata.clone());
        cmd_map.insert(cmd_key, metadata.clone()); // 新增
        true.into()
    }
}

/// 通过名称获取插件
#[register_hook(fallback = "TaggedOption::None")]
pub fn get_plugin_impl(
    name: &repr_c::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>> {
    let map = PLUGIN_MAP.lock().unwrap();
    match map.get(&name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 通过 command_name 获取插件
#[register_hook(fallback = "TaggedOption::None")]
pub fn get_plugin_by_command_name_impl(
    command_name: &repr_c::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>> {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    match cmd_map.get(&command_name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 删除插件
#[register_hook]
pub fn unregister_plugin_impl(name: &repr_c::String) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();

    // 先从 name map 取出 metadata 以获取 command_name
    if let Some(meta) = map.remove(&name.to_string()) {
        cmd_map.remove(&meta.command_name.to_string());
        true.into()
    } else {
        false.into()
    }
}

/// 检查插件是否存在
#[register_hook]
pub fn has_plugin_impl(name: &repr_c::String) -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&name.to_string()).into()
}

/// 获取插件数量
#[register_hook]
pub fn plugin_count_impl() -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有插件
#[register_hook]
pub fn clear_plugins_impl() {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    map.clear();
    cmd_map.clear();
}
