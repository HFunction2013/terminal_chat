use safer_ffi::prelude::*;

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