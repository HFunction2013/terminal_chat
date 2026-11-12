// init.rs
use cli_core_types::Result;
use hook_macro::register_hook;
use safer_ffi::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

pub static PROGRESS: AtomicU64 = AtomicU64::new(0);

#[register_hook]
pub fn add_progress() -> Result {
    PROGRESS.fetch_add(1, Ordering::Relaxed);
    Result::success()
}

#[register_hook]
pub fn add_progress_with(x: &u64) -> Result {
    PROGRESS.fetch_add(*x, Ordering::Relaxed);
    Result::success()
}

#[register_hook]
pub fn set_progress(x: &u64) -> Result {
    PROGRESS.store(*x, Ordering::Relaxed);
    Result::success()
}

#[register_hook]
pub fn get_progress(out: Out<'_, u64>) -> Result {
    out.write(PROGRESS.load(Ordering::Relaxed));
    Result::success()
}
