use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static IN_CMD: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn is_interrupted() -> Result {
    if INTERRUPTED.load(Ordering::SeqCst) {
        Result::success()
    } else {
        Result::error("not interrupted")
    }
}

#[ffi_export]
pub fn is_in_cmd() -> Result {
    if IN_CMD.load(Ordering::SeqCst) { Result::success() } else { Result::error("not in cmd") }
}

#[ffi_export]
pub fn set_interrupted(val: bool) -> Result {
    INTERRUPTED.store(val, Ordering::SeqCst);
    Result::success()
}

#[ffi_export]
pub fn set_in_cmd(val: bool) -> Result {
    IN_CMD.store(val, Ordering::SeqCst);
    Result::success()
}
