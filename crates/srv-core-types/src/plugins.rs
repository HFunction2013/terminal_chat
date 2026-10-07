//! 服务端插件（srv plugin）相关类型。
//!
//! 与 `cli-core-types` 的插件类型对称，但面向服务端插件：
//! srv-standard 等插件通过 `load_plugin` 被 srv-core 加载，
//! 在 `on_init_plugin` 里拿到宿主（srv-core）路径并注册 channel / packet handler。

use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;

/// srv-core 提供给插件（srv plugin）的宿主元数据。
///
/// `srv_core_path` 是 srv-core 动态库的绝对路径，插件用它
/// `libloading` 打开宿主库以获取 `register_channel` 等符号。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SrvHostMetadata {
    pub version: safer_ffi::String,
    pub srv_core_path: safer_ffi::String,
}

/// 服务端插件的元数据。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SrvPluginMetadata {
    pub name: safer_ffi::String,
    pub dylib_name: safer_ffi::String,
    pub version: safer_ffi::String,
    pub author: safer_ffi::String,
    pub description: safer_ffi::String,
    pub homepage: safer_ffi::String,
    pub license: safer_ffi::String,
    pub min_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
    pub max_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
}

/// 插件初始化/关闭的返回值。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SrvPluginResult {
    pub success: i32,
    pub exit_code: i32,
    pub msg: TaggedOption<safer_ffi::String>,
}

impl Default for SrvPluginResult {
    fn default() -> Self {
        Self { success: 0, exit_code: 0, msg: None.into() }
    }
}
