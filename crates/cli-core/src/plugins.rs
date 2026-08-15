use cli_core_types::{HostMetadata, PluginMetadata, PluginResult};
use hook_macro::register_hook;
use libloading::Library;
use process_path::get_dylib_path;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use which_dylib::{FindError, FindLibBuilder};

static PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static COMMAND_PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[register_hook]
pub fn is_plugin_name_registered(identifier: &safer_ffi::String) -> bool {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&identifier.to_string())
}

#[register_hook]
pub fn is_plugin_command_name_registered(identifier: &safer_ffi::String) -> bool {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    cmd_map.contains_key(&identifier.to_string())
}

#[register_hook]
pub fn load_plugin(plugin_name: &safer_ffi::String) -> PluginResult {
    unsafe {
        match FindLibBuilder::new().find_result(plugin_name) {
            Ok(plugin_lib) => {
                let plugin_path =
                    plugin_lib.to_str().expect("Failed to convert `PathBuf` to `&str`");
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

                let get_plugin_metadata = match lib
                    .get::<unsafe extern "C" fn() -> PluginMetadata>(b"get_plugin_metadata")
                {
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
                let on_init_plugin = match lib
                    .get::<unsafe extern "C" fn(HostMetadata) -> PluginResult>(b"on_init_plugin")
                {
                    Ok(f) => f,
                    Err(_) => {
                        // Err loading.
                        return PluginResult {
                            success: 0,
                            exit_code: 102,
                            msg: TaggedOption::Some("Failed to get `on_init_plugin`".into()),
                        };
                    }
                };
                if let Some(dylib_path) = get_dylib_path()
                    && let Some(dylib_path_str) = dylib_path.to_str()
                {
                    let res = on_init_plugin(HostMetadata {
                        version: env!("CARGO_PKG_VERSION").into(),
                        cli_core_path: dylib_path_str.into(),
                    });
                    if res.success != 1 {
                        return res;
                    }
                } else {
                    return PluginResult {
                        success: 0,
                        exit_code: 99,
                        msg: TaggedOption::Some("Failed to get `cli_core`'s path.".into()),
                    };
                }

                register_plugin(&meta);
                PluginResult {
                    success: 1,
                    exit_code: 0,
                    msg: TaggedOption::Some("Plugin loaded successfully.".into()),
                }
            }
            Err(err) => match err {
                FindError::NotFound(s) => PluginResult {
                    success: 0,
                    exit_code: 100,
                    msg: TaggedOption::Some(format!("NotFoundError: {}", s).into()),
                },
                FindError::Ambiguous(v) => PluginResult {
                    success: 0,
                    exit_code: 99,
                    msg: TaggedOption::Some(format!("AmbiguousError: {:?}", v).into()),
                },
            },
        }
    }
}

/// 注册插件（同时以 name 和 command_name 为键）
#[register_hook]
pub fn register_plugin(metadata: &PluginMetadata) -> i32 {
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
pub fn get_plugin(
    name: &safer_ffi::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>> {
    let map = PLUGIN_MAP.lock().unwrap();
    match map.get(&name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 通过 command_name 获取插件
#[register_hook(fallback = "TaggedOption::None")]
pub fn get_plugin_by_command_name(
    command_name: &safer_ffi::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>> {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    match cmd_map.get(&command_name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 删除插件
#[register_hook]
pub fn unregister_plugin(name: &safer_ffi::String) -> i32 {
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
pub fn has_plugin(name: &safer_ffi::String) -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&name.to_string()).into()
}

/// 获取插件数量
#[register_hook]
pub fn plugin_count() -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有插件
#[register_hook]
pub fn clear_plugins() {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    map.clear();
    cmd_map.clear();
}

#[register_hook(fallback = "safer_ffi::Vec::from(std::vec::Vec::new())")]
pub fn get_all_plugins() -> safer_ffi::Vec<PluginMetadata> {
    let map = PLUGIN_MAP.lock().unwrap();
    let plugins: Vec<PluginMetadata> = map.values().cloned().collect();
    plugins.into()
}
