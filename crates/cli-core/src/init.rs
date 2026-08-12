// init.rs
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use hook_macro::register_hook;
use safer_ffi::ffi_export;
pub static PROGRESS: AtomicU64 = AtomicU64::new(0);

#[register_hook]
pub fn add_progress_impl() {
    PROGRESS.fetch_add(1, Ordering::Relaxed);
}

#[register_hook]
pub fn add_progress_with_impl(x: u64) {
    PROGRESS.fetch_add(x, Ordering::Relaxed);
}

#[register_hook]
pub fn set_progress_impl(x: u64) {
    PROGRESS.store(x, Ordering::Relaxed);
}

#[register_hook]
pub fn get_progress_impl() -> u64 {
    PROGRESS.load(Ordering::Relaxed)
}
