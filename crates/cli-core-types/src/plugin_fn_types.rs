use crate::*;
use safer_ffi::prelude::Out;

/// 插件上报元数据：宿主提供出参缓冲，插件写入后以 Result 回报状态。
pub type GetPluginMetadataFn = extern "C" fn(Out<'_, PluginMetadata>) -> Result;
pub type RunCommandFn = extern "C" fn(&safer_ffi::Vec<safer_ffi::String>) -> Result;
pub type OnInitPluginFn = extern "C" fn(HostMetadata) -> PluginResult;
pub type OnShutdownPlugin = extern "C" fn() -> PluginResult;

/// 插件列举远端目录条目名：宿主传入目录路径与出参缓冲，插件经 RPC 填入条目名，
/// 以 Result 回报状态。供 TUI 路径补全同步取候选。
pub type ListRemoteDirFn =
    extern "C" fn(&safer_ffi::String, Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result;
