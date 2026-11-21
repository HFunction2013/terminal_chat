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

/// 插件回报当前 TCP 连接态：1=已连接，0=未连接。core hook 内部转发到插件时使用。
pub type PluginIsConnectedFn = extern "C" fn() -> i32;

/// 插件参数补全回调（可选导出符号 `tc_complete_arg`）。
///
/// 仅用于插件自定义概念——core 内置类型（成员 / 远端路径 / 本地路径 / 选项 key /
/// 插件 / 命名空间等）由宿主按参数声明类型补全；只有宿主不认识的新类型才走本回调。
/// 宿主传入拆分后的整行 argv、参数位置（1 起）与当前前缀，插件经出参返回候选字符串。
pub type CompleteArgFn = extern "C" fn(
    &safer_ffi::Vec<safer_ffi::String>,
    i32,
    &safer_ffi::String,
    Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result;
