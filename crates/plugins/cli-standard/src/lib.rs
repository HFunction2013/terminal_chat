use cli_core_types::Result as CmdResult;
use cli_core_types::{HostMetadata, LookupI18nFn, PluginMetadata, PluginResult};
use libloading::Library;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;
use std::sync::LazyLock;
use std::sync::OnceLock;

#[allow(dead_code)] // 生成模块的 t / t_lang / Lang 保留供测试与兜底，生产路径走 `tr()`。
mod i18n {
    include!("./trans.rs");
}

/// core `lookup_i18n` 符号缓存；旧核心无此符号时为 `None`，译文回退 key 本身。
static LOOKUP_I18N: OnceLock<Option<LookupI18nFn>> = OnceLock::new();

fn lookup_fn() -> Option<LookupI18nFn> {
    *LOOKUP_I18N.get_or_init(|| {
        let lib = LIB.get()?;
        unsafe { lib.get::<LookupI18nFn>(b"lookup_i18n\0").ok().map(|s| *s) }
    })
}

/// 插件侧翻译辅助：经 core `lookup_i18n` 按当前语言取译文；符号缺失 / 未知 key
/// 回退 key 本身。供本批次 UI 与下一批命令提示复用。
pub(crate) fn tr(key: &str) -> String {
    let Some(f) = lookup_fn() else { return key.to_string() };
    let mut buf: ManuallyDrop<safer_ffi::String> =
        ManuallyDrop::new(safer_ffi::String::from(String::new()));
    let _ = f(&key.into(), Out::from(&mut buf));
    ManuallyDrop::into_inner(buf).to_string()
}

static METADATA: LazyLock<PluginMetadata> = LazyLock::new(|| PluginMetadata {
    command_yaml: include_str!("../commands.yaml").into(),
    name: "cli-standard".into(),
    dylib_name: "cli_standard".into(),
    command_name: "std".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "cli standard commands.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::Some("0.1.0".into()),
    max_host_version: TaggedOption::None,
    pb_len: 0,
});

static LIB: OnceLock<Library> = OnceLock::new();

fn init_library(path: &str) -> &Library {
    LIB.get_or_init(|| unsafe { Library::new(path).expect("Failed to load library") })
}

mod cmd_i18n;
mod commands;
mod connection;
pub mod editor;
mod forms;
pub mod local;
mod menu;
mod session;
mod sidebar;
mod statusbar;
mod terminal;
mod command {
    use std::sync::LazyLock;

    use crate::METADATA;

    include!(concat!(env!("OUT_DIR"), "/command.rs"));
    pub static CMD: LazyLock<Command> = LazyLock::new(|| {
        let cmd = add_commands(
            Command::new(&*METADATA.command_name)
                .version(&*METADATA.version)
                .author(&*METADATA.author)
                .about(&*METADATA.description),
        );
        crate::cmd_i18n::localize_commands(cmd)
    });
}
#[ffi_export]
pub fn get_plugin_metadata(out: Out<'_, PluginMetadata>) -> CmdResult {
    out.write(METADATA.clone());
    CmdResult::success()
}

/// 本插件认识的内置选项 key（与代码读取处保持同步）。
const BUILTIN_OPTIONS: &[&str] =
    &["SERVER", "PORT", "USERNAME", "QUIET", "PREFER_COMMAND_LINE", "EDITOR", "LANGUAGE"];

#[ffi_export]
pub fn on_init_plugin(h_meta: HostMetadata) -> PluginResult {
    let lib = init_library(&h_meta.cli_core_path);
    // 把内置选项登记进宿主注册表；未注册的 key 在 set/setg 时会被拒绝。
    cli_core_macros::load_core_symbols!(lib, register_option, register_i18n_entry);
    for key in BUILTIN_OPTIONS {
        let _ = register_option(&safer_ffi::String::from(*key));
    }
    // 上报编译期词典条目；归属由 core 初始化上下文记为 `cli-standard`，
    // 卸载时随 `unregister_by_owner` 一并清理，插件侧不手写清理表。
    for (key, en, zh) in crate::i18n::entries() {
        let _ = register_i18n_entry(
            &safer_ffi::String::from(*key),
            &safer_ffi::String::from(*en),
            &safer_ffi::String::from(*zh),
        );
    }
    menu::register_menu(lib);
    forms::register_forms(lib);
    forms::register_form_menu_items(lib);
    sidebar::register_sidebar(lib);
    statusbar::register_statusbar(lib);
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

/// 取走异步推送队列（QUIET on 时静默），供 cli 主程序后台线程显示。
#[ffi_export]
pub fn cli_take_pushes() -> safer_ffi::Vec<safer_ffi::String> {
    crate::connection::take_pushes().into_iter().map(|s| s.into()).collect::<Vec<_>>().into()
}

#[ffi_export]
pub fn run_command(args: &safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
    // `run_command_impl`` will get `std` added;
    // let full_args: Vec<String> =
    //     std::iter::once("tc-cli".to_string()).chain(args.iter().map(|s| s.to_string())).collect();

    let full_args_refs: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();

    match (*command::CMD).clone().try_get_matches_from(&full_args_refs) {
        Ok(matches) => {
            // IN_CMD.store(true, Ordering::SeqCst);
            // cli_core will store it.
            // let _guard = CommandGuard;
            let result = commands::dispatch(&matches);
            match result {
                Ok(()) => CmdResult::success(),
                Err(e) => CmdResult::error(&e.to_string()),
            }
        }
        Err(err) => CmdResult::error(&err.to_string()),
    }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

/// 列举远端目录条目名（供 TUI 路径补全同步取候选）。失败或目录为空时回填空列表。
#[ffi_export]
pub fn list_remote_dir(
    dir: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> CmdResult {
    let req = protos::remote::fs::LsRequest { dir: dir.to_string() };
    let resp: protos::remote::fs::LsResponse = match crate::connection::rpc("remote_ls", &req) {
        Ok(r) => r,
        Err(e) => {
            out.write(Vec::new().into());
            return CmdResult::error(&e.to_string());
        }
    };
    let names: Vec<safer_ffi::String> = if resp.success {
        resp.entries
            .iter()
            .map(|e| {
                let name = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
                name.into()
            })
            .collect()
    } else {
        Vec::new()
    };
    out.write(names.into());
    CmdResult::success()
}

/// 查询当前是否已建立 TCP 连接（1=已连接，0=未连接），供 TUI 闸门判断。
#[ffi_export]
pub fn is_connected() -> i32 {
    crate::connection::CONNECTION.lock().unwrap().is_some() as i32
}

#[cfg(test)]
mod i18n_tests {
    use super::i18n::{Lang, t_lang};

    #[test]
    fn prompt_entries_lookup_both_languages() {
        assert_eq!(t_lang(Lang::En, "std.prompt.task.cancelled"), "[*] Task #{id} cancelled.");
        assert_eq!(t_lang(Lang::Zh, "std.prompt.task.cancelled"), "[*] 任务 #{id} 已取消。");
    }

    #[test]
    fn unknown_key_falls_back_to_key() {
        assert_eq!(t_lang(Lang::En, "std.prompt.missing.key"), "std.prompt.missing.key");
        assert_eq!(t_lang(Lang::Zh, "std.prompt.missing.key"), "std.prompt.missing.key");
    }

    #[test]
    fn help_metadata_entries_lookup_both_languages() {
        assert_eq!(t_lang(Lang::En, "std.help.setg.about"), "Set global options");
        assert_eq!(t_lang(Lang::Zh, "std.help.setg.about"), "设置全局选项");
        assert_eq!(t_lang(Lang::En, "std.help.create.alias.cmd.help"), "alias source");
        assert_eq!(t_lang(Lang::Zh, "std.help.create.alias.cmd.help"), "别名来源命令");
    }
}
