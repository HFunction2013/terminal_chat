use ::safer_ffi::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static IN_CMD: AtomicBool = AtomicBool::new(false);

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
