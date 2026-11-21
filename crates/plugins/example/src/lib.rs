use cli_core_types::{HostMetadata, PluginMetadata, PluginResult, Result as CmdResult};
use libloading::Library;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::sync::{LazyLock, OnceLock};

mod commands;

static METADATA: LazyLock<PluginMetadata> = LazyLock::new(|| PluginMetadata {
    command_yaml: include_str!("../commands.yaml").into(),
    name: "example".into(),
    dylib_name: "example".into(),
    command_name: "example".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "minimal example client plugin.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::Some("0.1.0".into()),
    max_host_version: TaggedOption::None,
    pb_len: 0,
});

static LIB: OnceLock<Library> = OnceLock::new();

fn init_library(path: &str) -> &'static Library {
    LIB.get_or_init(|| unsafe { Library::new(path).expect("Failed to load library") })
}

mod command {
    use crate::METADATA;
    use std::sync::LazyLock;

    include!(concat!(env!("OUT_DIR"), "/command.rs"));
    pub static CMD: LazyLock<clap::Command> = LazyLock::new(|| {
        add_commands(
            clap::Command::new(&*METADATA.command_name)
                .version(&*METADATA.version)
                .about(&*METADATA.description),
        )
    });
}

#[ffi_export]
pub fn get_plugin_metadata(out: Out<'_, PluginMetadata>) -> CmdResult {
    out.write(METADATA.clone());
    CmdResult::success()
}

#[ffi_export]
pub fn on_init_plugin(h_meta: HostMetadata) -> PluginResult {
    let lib = init_library(&h_meta.cli_core_path);
    // Example: register an option key this plugin accepts.
    cli_core_macros::load_core_symbols!(lib, register_option);
    let _ = register_option(&safer_ffi::String::from("EXAMPLE_OPTION"));
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn run_command(args: &safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
    let refs: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    match command::CMD.clone().try_get_matches_from(refs) {
        Ok(matches) => match commands::dispatch(&matches) {
            Ok(()) => CmdResult::success(),
            Err(e) => CmdResult::error(&e.to_string()),
        },
        Err(err) => CmdResult::error(&err.to_string()),
    }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

/// 可选参数补全回调（core 符号 `complete_arg` 按首词归属路由到本插件）。
///
/// 仅服务于插件自定义概念：yaml 中 `type: GREETING_NAME` 是宿主不认识的新类型，
/// 候选由本回调给出。内置类型（成员 / 远端路径 / 本地路径 / 选项 key / 插件 /
/// 命名空间）由宿主按声明类型自行补全，不需要实现本回调。
///
/// 入参：拆分后的整行 `argv`、参数位置（1 起）、当前前缀；出参写入候选字符串。
#[ffi_export]
pub fn tc_complete_arg(
    argv: &safer_ffi::Vec<safer_ffi::String>,
    arg_index: i32,
    prefix: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> CmdResult {
    let _ = (argv, arg_index);
    let needle = prefix.to_string();
    let names = ["alice", "bob", "carol", "world"];
    let rows: Vec<safer_ffi::String> =
        names.iter().copied().filter(|n| n.starts_with(&needle)).map(|s| s.into()).collect();
    out.write(rows.into());
    CmdResult::success()
}
