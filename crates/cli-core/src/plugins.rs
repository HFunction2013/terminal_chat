use cli_core_types::{
    HostMetadata, PluginMetadata, PluginResult, Result, plugin_fn_types::GetPluginMetadataFn,
    plugin_fn_types::ListRemoteDirFn, plugin_fn_types::OnInitPluginFn,
    plugin_fn_types::PluginIsConnectedFn,
};
use hook_macro::register_hook;
use libloading::Library;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use which_dylib::{FindError, FindLibBuilder};

static PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static COMMAND_PLUGIN_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 已加载插件库的句柄表：插件名 → Library。
///
/// 必须持有句柄：若 `load_plugin` 返回后句柄被 `dlclose` /
/// `FreeLibrary`，后续执行命令重新加载插件会得到新句柄，插件
/// 内部的 static（如 cli-standard 的 `LIB`）会重新初始化，
/// 导致 "cli-core not initialized"。`run_command` 必须复用这里
/// 保留的同一个句柄——Linux 下同路径 dlopen 复用句柄，但
/// Windows 下每次 LoadLibrary 都是新句柄，因此统一从这里取。
static PLUGIN_LIBS: LazyLock<Mutex<HashMap<String, Arc<Library>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
fn check_host_version(meta: &PluginMetadata) -> Option<PluginResult> {
    let host_ver = env!("CARGO_PKG_VERSION");
    let mut bound = None;
    if let TaggedOption::Some(v) = &meta.min_host_version {
        if cmp_version(host_ver, &v.to_string()) < 0 {
            bound = Some(format!(
                "plugin `{}` requires host >= {} but host is {}",
                meta.name,
                v.to_string(),
                host_ver
            ));
        }
    }
    if bound.is_none()
        && let TaggedOption::Some(v) = &meta.max_host_version
        && cmp_version(host_ver, &v.to_string()) > 0
    {
        bound = Some(format!(
            "plugin `{}` requires host <= {} but host is {}",
            meta.name,
            v.to_string(),
            host_ver
        ));
    }
    bound.map(|m| PluginResult { success: 0, exit_code: 103, msg: TaggedOption::Some(m.into()) })
}

/// （内部）取已加载插件的库句柄（Arc 克隆，生命周期不依赖锁）。
pub(crate) fn get_plugin_library(name: &str) -> Option<Arc<Library>> {
    PLUGIN_LIBS.lock().unwrap().get(name).cloned()
}

/// （内部）命令名是否已注册（供宿主命令分发直读，不经 FFI 出参包装）。
pub(crate) fn command_name_registered(cmd: &str) -> bool {
    COMMAND_PLUGIN_MAP.lock().unwrap().contains_key(cmd)
}

/// （内部）按 command_name 取插件元数据克隆（供宿主命令分发直读）。
pub(crate) fn plugin_by_command_name(cmd: &str) -> Option<PluginMetadata> {
    COMMAND_PLUGIN_MAP.lock().unwrap().get(cmd).cloned()
}

#[register_hook]
pub fn is_plugin_name_registered(identifier: &safer_ffi::String) -> Result {
    let map = PLUGIN_MAP.lock().unwrap();
    if map.contains_key(&identifier.to_string()) {
        Result::success()
    } else {
        Result::error("not found")
    }
}

#[register_hook]
pub fn is_plugin_command_name_registered(identifier: &safer_ffi::String) -> Result {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    if cmd_map.contains_key(&identifier.to_string()) {
        Result::success()
    } else {
        Result::error("not found")
    }
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
                    .get::<GetPluginMetadataFn>(b"get_plugin_metadata")
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
                let mut meta_buf: ManuallyDrop<PluginMetadata> =
                    ManuallyDrop::new(PluginMetadata::default());
                let _meta_res = get_plugin_metadata(Out::from(&mut meta_buf));
                let meta: PluginMetadata = ManuallyDrop::into_inner(meta_buf);
                // 检查插件声明的 host 版本范围。
                if let Some(err) = check_host_version(&meta) {
                    return err;
                }
                let on_init_plugin = match lib.get::<OnInitPluginFn>(b"on_init_plugin") {
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
                let core_path = match FindLibBuilder::new().find_result("cli_core") {
                    Ok(p) => p,
                    Err(_) => {
                        return PluginResult {
                            success: 0,
                            exit_code: 99,
                            msg: TaggedOption::Some("Failed to get `cli_core`'s path.".into()),
                        };
                    }
                };
                let core_path_str =
                    core_path.to_str().expect("Failed to convert `PathBuf` to `&str`");
                let res = on_init_plugin(HostMetadata {
                    version: env!("CARGO_PKG_VERSION").into(),
                    cli_core_path: core_path_str.into(),
                });
                if res.success != 1 {
                    return res;
                }

                let _ = register_plugin(&meta);
                // 持有插件库句柄，run_command 复用同一句柄（跨平台）。
                PLUGIN_LIBS.lock().unwrap().insert(meta.name.to_string(), Arc::new(lib));
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
pub fn register_plugin(metadata: &PluginMetadata) -> Result {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();

    let key = metadata.name.to_string();
    let cmd_key = metadata.command_name.to_string();

    if map.contains_key(&key) || cmd_map.contains_key(&cmd_key) {
        Result::error("already registered")
    } else {
        map.insert(key, metadata.clone());
        cmd_map.insert(cmd_key, metadata.clone());
        Result::success()
    }
}

/// 通过名称获取插件
#[register_hook]
pub fn get_plugin(
    name: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>>>,
) -> Result {
    let map = PLUGIN_MAP.lock().unwrap();
    let value = match map.get(&name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    };
    out.write(value);
    Result::success()
}

/// 通过 command_name 获取插件
#[register_hook]
pub fn get_plugin_by_command_name(
    command_name: &safer_ffi::String,
    out: Out<'_, TaggedOption<safer_ffi::boxed::ThinBox<PluginMetadata>>>,
) -> Result {
    let cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    let value = match cmd_map.get(&command_name.to_string()) {
        Some(p) => TaggedOption::Some(safer_ffi::boxed::ThinBox::new(p.clone())),
        None => TaggedOption::None,
    };
    out.write(value);
    Result::success()
}

/// 删除插件
#[register_hook]
pub fn unregister_plugin(name: &safer_ffi::String) -> Result {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();

    // 先从 name map 取出 metadata 以获取 command_name
    if let Some(meta) = map.remove(&name.to_string()) {
        cmd_map.remove(&meta.command_name.to_string());
        // 注意：不能移除 PLUGIN_LIBS 句柄——dlclose 正在执行的插件（如
        // cli-standard）会使后续指令落在已卸载的代码页上导致段错误。
        // 注册表移除后命令即失效，句柄随进程退出统一释放。
        Result::success()
    } else {
        Result::error("not found")
    }
}

/// 检查插件是否存在
#[register_hook]
pub fn has_plugin(name: &safer_ffi::String) -> Result {
    let map = PLUGIN_MAP.lock().unwrap();
    if map.contains_key(&name.to_string()) { Result::success() } else { Result::error("not found") }
}

/// 获取插件数量（经出参回填；状态经 Result.code 表达）。
#[register_hook]
pub fn plugin_count(out: Out<'_, i32>) -> Result {
    let map = PLUGIN_MAP.lock().unwrap();
    out.write(map.len() as i32);
    Result::success()
}

/// 清空所有插件
#[register_hook]
pub fn clear_plugins() -> Result {
    let mut map = PLUGIN_MAP.lock().unwrap();
    let mut cmd_map = COMMAND_PLUGIN_MAP.lock().unwrap();
    let mut libs = PLUGIN_LIBS.lock().unwrap();
    map.clear();
    cmd_map.clear();
    libs.clear();
    Result::success()
}

#[register_hook]
pub fn get_all_plugins(out: Out<'_, safer_ffi::Vec<PluginMetadata>>) -> Result {
    let map = PLUGIN_MAP.lock().unwrap();
    let plugins: Vec<PluginMetadata> = map.values().cloned().collect();
    out.write(plugins.into());
    Result::success()
}

/// 列举远端目录条目名：转发到已加载 cli-standard 插件的 RPC 通路，供 TUI 路径补全。
/// 插件未加载或符号缺失时回填空列表，不阻断补全。
#[register_hook]
pub fn remote_list_dir(
    dir: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result {
    let Some(lib) = get_plugin_library("cli-standard") else {
        out.write(Vec::new().into());
        return Result::success();
    };
    let list_dir: ListRemoteDirFn = unsafe {
        match lib.get(b"list_remote_dir") {
            Ok(f) => *f,
            Err(_) => {
                out.write(Vec::new().into());
                return Result::success();
            }
        }
    };
    list_dir(dir, out);
    Result::success()
}

/// 查询插件当前 TCP 连接态（1=已连接，0=未连接）：转发到 cli-standard 插件；
/// 插件未加载或符号缺失时回写 0。
#[register_hook]
pub fn is_connected(out: Out<'_, i32>) -> Result {
    let Some(lib) = get_plugin_library("cli-standard") else {
        out.write(0);
        return Result::success();
    };
    let f: PluginIsConnectedFn = unsafe {
        match lib.get(b"is_connected") {
            Ok(s) => *s,
            Err(_) => {
                out.write(0);
                return Result::success();
            }
        }
    };
    out.write(f());
    Result::success()
}
