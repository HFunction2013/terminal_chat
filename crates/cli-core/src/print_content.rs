use ::safer_ffi::prelude::*;
use hook_macro::register_hook;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

#[register_hook]
fn print_content(content: &safer_ffi::String) {
    println!("{}", &**content);
}
