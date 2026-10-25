//! 服务端插件管理。
//!
//! 仿照 `cli-core::plugins`：srv-standard 等服务端插件是 cdylib，
//! 导出 `get_plugin_metadata` 与 `on_init_plugin`。`load_plugin`
//! 用 `which_dylib` 定位插件库、`libloading` 加载，并在
//! `on_init_plugin` 中把宿主（srv-core）路径交给插件，插件借此
//! 注册频道与 packet handler。

use libloading::Library;
use process_path::get_dylib_path;
use safer_ffi::option::TaggedOption;
use srv_core_types::init_fn_types::{GetPluginMetadataFn, OnInitPluginFn};
use srv_core_types::plugins::{
    SrvHostMetadata, SrvPluginMetadata, SrvPluginResult,
};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use which_dylib::{FindError, FindLibBuilder};
use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};


static PLUGIN_MAP: LazyLock<Mutex<HashMap<String, SrvPluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 已加载插件库的句柄表：插件名 → Library。
///
/// 必须持有句柄，否则 `load_plugin` 返回后动态库被 `dlclose`，
/// 插件注册到 srv-core 的函数指针（channel handler 等）会变成
/// 悬空指针，调用时崩溃。
static PLUGIN_LIBS: LazyLock<Mutex<HashMap<String, Library>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 插件名是否已注册。
#[register_hook]
pub fn is_plugin_registered(identifier: &safer_ffi::String) -> bool {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&identifier.to_string())
}

/// 简单语义化版本比较：a < b → -1，a == b → 0，a > b → 1。
fn cmp_version(a: &str, b: &str) -> i32 {
    let pa: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x < y {
            return -1;
        }
        if x > y {
            return 1;
        }
    }
    0
}

/// 校验插件声明的 host 版本范围（min_host_version / max_host_version）。
fn check_host_version(meta: &SrvPluginMetadata) -> Option<SrvPluginResult> {
    let host_ver = env!("CARGO_PKG_VERSION");
    let mut bound = None;
    if let TaggedOption::Some(v) = &meta.min_host_version {
        if cmp_version(host_ver, &v.to_string()) < 0 {
            bound = Some(format!(
                "plugin `{}` requires host >= {} but host is {}",
                meta.name, v.to_string(), host_ver
            ));
        }
    }
    if bound.is_none()
        && let TaggedOption::Some(v) = &meta.max_host_version
        && cmp_version(host_ver, &v.to_string()) > 0
    {
        bound = Some(format!(
            "plugin `{}` requires host <= {} but host is {}",
            meta.name, v.to_string(), host_ver
        ));
    }
    bound.map(|m| SrvPluginResult {
        success: 0,
        exit_code: 103,
        msg: TaggedOption::Some(m.into()),
    })
}

/// 加载服务端插件（如 `"srv_standard"`）。
///
/// 流程与 cli-core 的 `load_plugin` 一致：
/// 定位 dylib → 取 `get_plugin_metadata` → 取 `on_init_plugin` →
/// 传入宿主元数据调用 → 注册插件。
#[register_hook]
pub fn load_plugin(plugin_name: &safer_ffi::String) -> SrvPluginResult {
    unsafe {
        match FindLibBuilder::new().find_result(plugin_name) {
            Ok(plugin_lib) => {
                let plugin_path =
                    plugin_lib.to_str().expect("Failed to convert `PathBuf` to `&str`");
                let lib = match Library::new(plugin_path.to_string()) {
                    Ok(lib) => lib,
                    Err(_) => {
                        return SrvPluginResult {
                            success: 0,
                            exit_code: 101,
                            msg: TaggedOption::Some("Failed to load plugin".into()),
                        };
                    }
                };

                let get_plugin_metadata = match lib
                    .get::<GetPluginMetadataFn>(b"get_plugin_metadata")
                {
                    Ok(f) => f,
                    Err(_) => {
                        return SrvPluginResult {
                            success: 0,
                            exit_code: 102,
                            msg: TaggedOption::Some(
                                "Failed to get `get_plugin_metadata`".into(),
                            ),
                        };
                    }
                };
                let meta: SrvPluginMetadata = get_plugin_metadata();
                // 检查插件声明的 host 版本范围。
                if let Some(err) = check_host_version(&meta) {
                    return err;
                }
                let on_init_plugin = match lib.get::<OnInitPluginFn>(b"on_init_plugin") {
                    Ok(f) => f,
                    Err(_) => {
                        return SrvPluginResult {
                            success: 0,
                            exit_code: 102,
                            msg: TaggedOption::Some("Failed to get `on_init_plugin`".into()),
                        };
                    }
                };
                if let Some(dylib_path) = get_dylib_path()
                    && let Some(dylib_path_str) = dylib_path.to_str()
                {
                    let res = on_init_plugin(SrvHostMetadata {
                        version: env!("CARGO_PKG_VERSION").into(),
                        srv_core_path: dylib_path_str.into(),
                    });
                    if res.success != 1 {
                        return res;
                    }
                } else {
                    return SrvPluginResult {
                        success: 0,
                        exit_code: 99,
                        msg: TaggedOption::Some("Failed to get `srv_core`'s path.".into()),
                    };
                }

                register_plugin(&meta);
                // 持有插件库句柄，防止 dlclose 后 handler 悬空。
                PLUGIN_LIBS.lock().unwrap().insert(meta.name.to_string(), lib);
                SrvPluginResult {
                    success: 1,
                    exit_code: 0,
                    msg: TaggedOption::Some("Plugin loaded successfully.".into()),
                }
            }
            Err(err) => match err {
                FindError::NotFound(s) => SrvPluginResult {
                    success: 0,
                    exit_code: 100,
                    msg: TaggedOption::Some(format!("NotFoundError: {}", s).into()),
                },
                FindError::Ambiguous(v) => SrvPluginResult {
                    success: 0,
                    exit_code: 99,
                    msg: TaggedOption::Some(format!("AmbiguousError: {:?}", v).into()),
                },
            },
        }
    }
}

/// 注册插件元数据（name 为唯一键）。
#[register_hook]
pub fn register_plugin(metadata: &SrvPluginMetadata) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let key = metadata.name.to_string();
    if map.contains_key(&key) {
        false.into()
    } else {
        map.insert(key, metadata.clone());
        true.into()
    }
}

/// 通过名称获取插件。
#[register_hook(fallback = "TaggedOption::None")]
pub fn get_plugin(
    name: &safer_ffi::String,
) -> TaggedOption<safer_ffi::boxed::ThinBox<SrvPluginMetadata>> {
    let map = PLUGIN_MAP.lock().unwrap();
    match map.get(&name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    }
}

/// 删除插件。
#[register_hook]
pub fn unregister_plugin(name: &safer_ffi::String) -> i32 {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let removed = map.remove(&name.to_string()).is_some();
    if removed {
        PLUGIN_LIBS.lock().unwrap().remove(&name.to_string());
    }
    removed.into()
}

/// 检查插件是否存在。
#[register_hook]
pub fn has_plugin(name: &safer_ffi::String) -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.contains_key(&name.to_string()).into()
}

/// 获取插件数量。
#[register_hook]
pub fn plugin_count() -> i32 {
    let map = PLUGIN_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有插件。
#[register_hook]
pub fn clear_plugins() {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut libs = PLUGIN_LIBS.lock().unwrap();
    map.clear();
    libs.clear();
}

/// 获取全部插件元数据。
#[register_hook(fallback = "safer_ffi::Vec::from(std::vec::Vec::new())")]
pub fn get_all_plugins() -> safer_ffi::Vec<SrvPluginMetadata> {
    let map = PLUGIN_MAP.lock().unwrap();
    let plugins: Vec<SrvPluginMetadata> = map.values().cloned().collect();
    plugins.into()
}
