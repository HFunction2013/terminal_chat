use cli_core::plugins::{PluginMetadata, PluginResult};
use cli_core::result::Result as CmdResult;
use safer_ffi::ffi_export;
#[ffi_export]
pub fn get_plugin_metadata() -> PluginMetadata {
    PluginMetadata {
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
    }
}

#[ffi_export]
pub fn on_init_plugin() -> PluginResult {
    PluginResult { success: true, exit_code: 0 }
}

#[ffi_export]
pub fn run_command(args: safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
    CmdResult::success()
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true, exit_code: 0 }
}
