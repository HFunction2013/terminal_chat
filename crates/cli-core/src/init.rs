// init.rs
use std::sync::atomic::{AtomicU64, Ordering};

use safer_ffi::ffi_export;
pub static PROGRESS: AtomicU64 = AtomicU64::new(0);

#[ffi_export]
pub fn add_progress() {
    PROGRESS.fetch_add(1, Ordering::Relaxed);
}

#[ffi_export]
pub fn add_progress_with(x: u64) {
    PROGRESS.fetch_add(x, Ordering::Relaxed);
}

#[ffi_export]
pub fn set_progress(x: u64) {
    PROGRESS.store(x, Ordering::Relaxed);
}

#[ffi_export]
pub fn get_progress() -> u64 {
    PROGRESS.load(Ordering::Relaxed)
}
