use ::safer_ffi::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod global_settings;
pub mod macros;
pub mod print_content;
pub mod result;
pub mod run_commands;
pub mod plugins;
pub mod init;

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static IN_CMD: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

#[ffi_export]
pub fn is_in_cmd() -> bool {
    IN_CMD.load(Ordering::SeqCst)
}

#[ffi_export]
pub fn set_interrupted(val: bool) {
    INTERRUPTED.store(val, Ordering::SeqCst);
}

#[ffi_export]
pub fn set_in_cmd(val: bool) {
    IN_CMD.store(val, Ordering::SeqCst);
}
