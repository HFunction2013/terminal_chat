use crate::*;
use safer_ffi::prelude::Out;

/// 插件上报元数据：宿主提供出参缓冲，插件写入后以 Result 回报状态。
pub type GetPluginMetadataFn = extern "C" fn(Out<'_, PluginMetadata>) -> Result;
pub type RunCommandFn = extern "C" fn(&safer_ffi::Vec<safer_ffi::String>) -> Result;
pub type OnInitPluginFn = extern "C" fn(HostMetadata) -> PluginResult;
pub type OnShutdownPlugin = extern "C" fn() -> PluginResult;
