use cli_core_types::{
    HostMetadata, PluginMetadata, PluginResult, Result, plugin_fn_types::GetPluginMetadataFn,
    plugin_fn_types::ListRemoteDirFn, plugin_fn_types::OnInitPluginFn,
    plugin_fn_types::OnShutdownPlugin, plugin_fn_types::PluginIsConnectedFn,
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

/// 全局命令名 → 插件元数据。
///
/// 插件 load 时把其 `command_yaml` 顶层命令名登记进此表；未带 namespace 前缀的
/// 命令先在此表解析归属插件，而不是在指令前补默认 namespace 前缀。
static GLOBAL_COMMAND_MAP: LazyLock<Mutex<HashMap<String, PluginMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 当前正在初始化（on_init_plugin）的插件归属，core 内部上下文。
///
/// 各注册原语在写入条目时据此记录 owner，卸载时按 owner 清理；不经 FFI 传递，
/// 因此不改动插件注册原语签名。
static CURRENT_OWNER: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

/// 记录当前初始化插件归属（`None` 清除）。仅 core 内部调用。
pub(crate) fn set_current_owner(name: Option<String>) {
    *CURRENT_OWNER.lock().unwrap() = name;
}

/// 读取当前归属插件名（core 内部）。
pub(crate) fn current_owner() -> Option<String> {
    CURRENT_OWNER.lock().unwrap().clone()
}

/// 解析插件 `command_yaml` 的顶层命令名（容错：解析失败返回空列表，不阻断加载）。
fn top_level_command_names(yaml: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct YamlDoc {
        commands: Vec<YamlCmd>,
    }
    #[derive(serde::Deserialize)]
    struct YamlCmd {
        name: String,
    }
    serde_yaml::from_str::<YamlDoc>(yaml)
        .map(|doc| doc.commands.into_iter().map(|cmd| cmd.name).collect())
        .unwrap_or_default()
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

/// （内部）按全局命令名取归属插件元数据克隆（供命令分发直读）。
pub(crate) fn global_plugin_by_command(cmd: &str) -> Option<PluginMetadata> {
    GLOBAL_COMMAND_MAP.lock().unwrap().get(cmd).cloned()
}

/// （内部）全部已加载插件的命令命名空间（`command_name`），排序去重；
/// 供顶层命令补全把可作为首词路由的 namespace 前缀一并列出。
pub(crate) fn all_command_namespaces() -> Vec<String> {
    let map = PLUGIN_MAP.lock().unwrap();
    let mut namespaces: Vec<String> = map.values().map(|m| m.command_name.to_string()).collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces
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
                set_current_owner(Some(meta.name.to_string()));
                let res = on_init_plugin(HostMetadata {
                    version: env!("CARGO_PKG_VERSION").into(),
                    cli_core_path: core_path_str.into(),
                });
                set_current_owner(None);
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
        // 把顶层命令名登记进全局命令表；同名命令先注册者优先，不覆盖。
        let mut global = GLOBAL_COMMAND_MAP.lock().unwrap();
        for name in top_level_command_names(&metadata.command_yaml.to_string()) {
            global.entry(name).or_insert_with(|| metadata.clone());
        }
        // 解析命令 / 子命令 / 参数规格写入统一命令表，供宿主命令与参数补全查询。
        crate::registered_commands::register_yaml(
            &metadata.command_yaml.to_string(),
            &metadata.name.to_string(),
        );
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

/// 删除插件：调用其 `on_shutdown_plugin`，移除元数据 / 命令表 / 全局命令表条目，
/// 并按归属清理该插件在各注册表登记的选项与 UI 条目。
#[register_hook]
pub fn unregister_plugin(name: &safer_ffi::String) -> Result {
    let meta = { PLUGIN_MAP.lock().unwrap().remove(&name.to_string()) };
    let Some(meta) = meta else { return Result::error("not found") };

    // 通知插件 shutdown。句柄仍保留在 PLUGIN_LIBS：dlclose 正在执行的插件会使
    // 后续指令落在已卸载代码页上导致段错误；注册表移除后命令即失效，句柄随进程退出释放。
    if let Some(lib) = get_plugin_library(&meta.name.to_string()) {
        if let Ok(shutdown) = unsafe { lib.get::<OnShutdownPlugin>(b"on_shutdown_plugin") } {
            let _ = shutdown();
        }
    }

    COMMAND_PLUGIN_MAP.lock().unwrap().remove(&meta.command_name.to_string());
    GLOBAL_COMMAND_MAP.lock().unwrap().retain(|_, m| m.name.to_string() != meta.name.to_string());

    // 按归属清理选项与 UI 注册项。
    crate::registered_options::unregister_by_owner(&meta.name.to_string());
    crate::registered_ui::unregister_by_owner(&meta.name.to_string());
    crate::registered_commands::unregister_by_owner(&meta.name.to_string());

    Result::success()
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
    PLUGIN_MAP.lock().unwrap().clear();
    COMMAND_PLUGIN_MAP.lock().unwrap().clear();
    GLOBAL_COMMAND_MAP.lock().unwrap().clear();
    PLUGIN_LIBS.lock().unwrap().clear();
    crate::registered_options::clear_all();
    crate::registered_ui::clear_all();
    crate::registered_commands::clear_all();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_command_table_registers_and_unregisters() {
        let mut meta = PluginMetadata::default();
        meta.name = "t-plug-global".into();
        meta.dylib_name = "t_plug_global".into();
        meta.command_name = "tns".into();
        meta.command_yaml = "commands:\n  - name: foo\n  - name: bar\n".into();

        unsafe {
            assert_eq!(register_plugin(&meta).code, 0);
            assert!(command_name_registered("tns"));
            assert!(global_plugin_by_command("foo").is_some());
            assert!(global_plugin_by_command("bar").is_some());

            assert_eq!(unregister_plugin(&"t-plug-global".into()).code, 0);
            assert!(!command_name_registered("tns"));
            assert!(global_plugin_by_command("foo").is_none());
            assert!(global_plugin_by_command("bar").is_none());
        }
    }
}
