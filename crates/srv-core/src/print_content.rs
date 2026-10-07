//! 服务端日志输出。

use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};


/// 输出一行服务端日志（当前直接打印到 stdout）。
#[register_hook]
fn print_content(content: &safer_ffi::String) {
    println!("{}", &**content);
}
