use cli_core_types::Result as CmdResult;
use cli_core_types::{HostMetadata, PluginMetadata, PluginResult};
use libloading::Library;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::sync::LazyLock;
use std::sync::OnceLock;

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

mod commands;
mod connection;
pub mod editor;
pub mod local;
mod session;
mod command {
    use std::sync::LazyLock;

    use crate::METADATA;

    include!(concat!(env!("OUT_DIR"), "/command.rs"));
    pub static CMD: LazyLock<Command> = LazyLock::new(|| {
        add_commands(
            Command::new(&*METADATA.command_name)
                .version(&*METADATA.version)
                .author(&*METADATA.author)
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
    init_library(&h_meta.cli_core_path);
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
