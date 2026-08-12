use cli_core::plugins::{PluginMetadata, PluginResult};
use cli_core::result::Result as CmdResult;
use safer_ffi::ffi_export;
static METADATA: PluginMetadata = PluginMetadata {
    command_yaml: include_str!("../commands.yaml"),
    name: "cli-standard",
    command_name: "std",
    version: env!("CARGO_PKG_VERSION"),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or(""),
    description: "cli standard commands.",
    homepage: "",
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED"),
    min_host_version: TaggedOption::None,
    max_host_version: TaggedOption::None,
    pb_len: 0,
};
mod command {
    use clap::Command;
    use std::sync::LazyLock;

    use crate::METADATA;

    include!(concat!(env!("OUT_DIR"), "/command.rs"));
    pub static CMD: LazyLock<Command> = LazyLock::new(add_command(
        Command::new(METADATA.command_name)
            .version(METADATA.version)
            .author(METADATA.author)
            .about(METADATA.description),
    ));
}
#[ffi_export]
pub fn get_plugin_metadata() -> PluginMetadata {
    METADATA
}

#[ffi_export]
pub fn on_init_plugin() -> PluginResult {
    PluginResult { success: true, exit_code: 0 }
}

#[ffi_export]
pub fn run_command(args: safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
    // `run_command_impl`` will get `std` added;
    // let full_args: Vec<String> =
    //     std::iter::once("tc-cli".to_string()).chain(args.iter().map(|s| s.to_string())).collect();

    let full_args_refs: Vec<&str> = full_args.iter().map(String::as_str).collect();

    match (*command::CMD).clone().try_get_matches_from(&full_args_refs) {
        Ok(matches) => {
            // IN_CMD.store(true, Ordering::SeqCst);
            // cli_core will store it.
            // let _guard = CommandGuard;
            let result = commands::dispatch(&matches);
            match result {
                Ok(()) => Result::success(),
                Err(e) => Result::error(&e.to_string()),
            }
        }
        Err(err) => Result::error(&err.to_string()),
    }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true, exit_code: 0 }
}
