use cli_core::plugins::{PluginMetadata, PluginResult};
use cli_core::result::Result as CmdResult;
use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
static METADATA: PluginMetadata = PluginMetadata {
    command_yaml: include_str!("../commands.yaml"),
    name: "cli-standard".into(),
    command_name: "std".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "cli standard commands.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::None,
    max_host_version: TaggedOption::None,
    pb_len: 0,
};
mod commands;
mod command {
    use clap::Command;
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
pub fn get_plugin_metadata() -> PluginMetadata {
    METADATA.clone()
}

#[ffi_export]
pub fn on_init_plugin() -> PluginResult {
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn run_command(args: safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
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
