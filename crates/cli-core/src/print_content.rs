use ::safer_ffi::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

// ===== print_content 钩子系统 =====
type BeforePrintContentHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnPrintContentHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterPrintContentHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_PRINT_CONTENT_HOOKS: LazyLock<Mutex<Vec<BeforePrintContentHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_PRINT_CONTENT_HOOKS: LazyLock<Mutex<Vec<OnPrintContentHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_PRINT_CONTENT_HOOKS: LazyLock<Mutex<Vec<AfterPrintContentHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_PRINT_CONTENT: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_print_content(hook: BeforePrintContentHook) {
    if let Ok(mut hooks) = BEFORE_PRINT_CONTENT_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_print_content(hook: OnPrintContentHook) {
    if let Ok(mut hooks) = ON_PRINT_CONTENT_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_print_content(hook: AfterPrintContentHook) {
    if let Ok(mut hooks) = AFTER_PRINT_CONTENT_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_print_content_hooks() {
    let _ = BEFORE_PRINT_CONTENT_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_PRINT_CONTENT_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_PRINT_CONTENT_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_print_content_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_PRINT_CONTENT_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *mut repr_c::String) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_print_content_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_PRINT_CONTENT_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const repr_c::String) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_print_content_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_PRINT_CONTENT_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const repr_c::String) {
                    return true;
                }
            }
        }
    }
    false
}

fn print_content_impl(content: &repr_c::String) {
    println!("{}", &**content);
}

#[ffi_export]
pub unsafe fn print_content(content: repr_c::String) {
    let reenter = IN_PRINT_CONTENT.load(Ordering::SeqCst);
    if reenter {
        print_content_impl(&content);
        return;
    }
    IN_PRINT_CONTENT.store(true, Ordering::SeqCst);
    
    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_print_content_chain(&mut buf) };
    
    if !interrupted {
        interrupted = unsafe { run_on_print_content_chain(&buf) };
    }
    
    if !interrupted {
        print_content_impl(&buf);
        unsafe { run_after_print_content_chain(&buf) };
    }
    
    IN_PRINT_CONTENT.store(false, Ordering::SeqCst);
}