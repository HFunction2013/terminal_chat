//! srv-core 内部状态查询函数类型（手动声明部分）。
//!
//! 与 `cli-core-types::init_fn_types` 对称。build.rs 会自动生成
//! `#[register_hook]` 函数的类型；这里是纯 `#[ffi_export]` 函数的
//! 类型声明，供 srv / 插件通过 libloading 按符号名获取。

use crate::Result;
use safer_ffi::prelude::Out;

/// 插件约定的 `get_plugin_metadata` 函数类型。
///
/// 宿主提供出参缓冲，插件写入元数据后以 `Result` 回报状态。
pub type GetPluginMetadataFn = extern "C" fn(Out<'_, crate::plugins::SrvPluginMetadata>) -> Result;

/// 插件约定的 `on_init_plugin` 函数类型。
pub type OnInitPluginFn =
    extern "C" fn(crate::plugins::SrvHostMetadata) -> crate::plugins::SrvPluginResult;

/// 插件约定的 `on_shutdown_plugin` 函数类型。
pub type OnShutdownPluginFn = extern "C" fn() -> crate::plugins::SrvPluginResult;
