use safer_ffi::prelude::*;
use safer_ffi::option::TaggedOption;

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct GlobalOption {
    pub key: safer_ffi::String,
    pub value: safer_ffi::String,
}

impl GlobalOption {
    pub fn new(key: &str, value: &str) -> Self {
        GlobalOption { key: key.into(), value: value.into() }
    }
}

impl Default for GlobalOption {
    fn default() -> Self {
        GlobalOption { key: "".into(), value: "".into() }
    }
}


#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: safer_ffi::String,
    pub code: safer_ffi::String,
}

impl MacroDef {
    pub fn new(name: &str, code: &str) -> Self {
        MacroDef { name: name.into(), code: code.into() }
    }
}

impl Default for MacroDef {
    fn default() -> Self {
        MacroDef { name: "".into(), code: "".into() }
    }
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct HostMetadata {
    pub version: safer_ffi::String,
    pub cli_core_path: safer_ffi::String,
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub command_yaml: safer_ffi::String,
    pub name: safer_ffi::String,
    pub dylib_name: safer_ffi::String,
    pub command_name: safer_ffi::String,
    pub version: safer_ffi::String,
    pub author: safer_ffi::String,
    pub description: safer_ffi::String,
    pub homepage: safer_ffi::String,
    pub license: safer_ffi::String,
    pub min_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
    pub max_host_version: safer_ffi::option::TaggedOption<safer_ffi::String>,
    pub pb_len: i32,
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub success: i32,
    pub exit_code: i32,
    pub msg: TaggedOption<safer_ffi::String>,
}

impl Default for PluginResult {
    fn default() -> Self {
        Self { success: 0, exit_code: 0, msg: None.into() }
    }
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Result {
    pub code: i32,
    pub message: safer_ffi::String,
}

impl Default for Result {
    fn default() -> Self {
        Result { code: -1, message: "Default Result".into() }
    }
}

impl Result {
    pub fn success() -> Self {
        Result { code: 0, message: safer_ffi::String::from(String::new()) }
    }

    pub fn error(msg: &str) -> Self {
        Result { code: 1, message: msg.into() }
    }
}
