use safer_ffi::prelude::*;

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
