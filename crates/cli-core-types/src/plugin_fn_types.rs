use crate::*;
pub type GetPluginMetadataFn = extern "C" fn() -> PluginMetadata;
pub type RunCommandFn = extern "C" fn(&safer_ffi::Vec<safer_ffi::String>) -> Result;
pub type OnInitPluginFn = extern "C" fn(HostMetadata) -> PluginResult;
pub type OnShutdownPlugin = extern "C" fn() -> PluginResult;
