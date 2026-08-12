use ::safer_ffi::prelude::*;
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Result {
    pub code: i32,
    pub message: repr_c::String,
}

impl Default for Result {
    fn default() -> Self {
        Result { code: -1, message: "Default Result".into() }
    }
}

impl Result {
    pub fn success() -> Self {
        Result { code: 0, message: repr_c::String::from(String::new()) }
    }

    pub fn error(msg: &str) -> Self {
        Result { code: 1, message: msg.into() }
    }
}
