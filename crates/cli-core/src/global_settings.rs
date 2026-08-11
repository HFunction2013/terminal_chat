use ::safer_ffi::prelude::*;
use safer_ffi::option::TaggedOption;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct GlobalOption {
    pub key: repr_c::String,
    pub value: repr_c::String,
}

impl GlobalOption {
    pub fn new(key: &str, value: &str) -> Self {
        GlobalOption { key: key.into(), value: value.into() }
    }
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ===== set_global_option 钩子系统 =====
type BeforeSetGlobalOptionHook = unsafe extern "C" fn(*mut GlobalOption) -> bool;
type OnSetGlobalOptionHook = unsafe extern "C" fn(*const GlobalOption) -> bool;
type AfterSetGlobalOptionHook = unsafe extern "C" fn(*const GlobalOption) -> bool;

static BEFORE_SET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<BeforeSetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_SET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<OnSetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_SET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<AfterSetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_SET_GLOBAL_OPTION: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_set_global_option(hook: BeforeSetGlobalOptionHook) {
    if let Ok(mut hooks) = BEFORE_SET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_set_global_option(hook: OnSetGlobalOptionHook) {
    if let Ok(mut hooks) = ON_SET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_set_global_option(hook: AfterSetGlobalOptionHook) {
    if let Ok(mut hooks) = AFTER_SET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_set_global_option_hooks() {
    let _ = BEFORE_SET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_SET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_SET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_set_global_option_chain(buf: &mut GlobalOption) -> bool {
    if let Ok(list) = BEFORE_SET_GLOBAL_OPTION_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *mut GlobalOption) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_set_global_option_chain(buf: &GlobalOption) -> bool {
    if let Ok(list) = ON_SET_GLOBAL_OPTION_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const GlobalOption) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_set_global_option_chain(buf: &GlobalOption) -> bool {
    if let Ok(list) = AFTER_SET_GLOBAL_OPTION_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const GlobalOption) {
                    return true;
                }
            }
        }
    }
    false
}

fn set_global_option_impl(option: &GlobalOption) -> GlobalOption {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.to_string(), option.value.to_string());
    option.clone()
}

#[ffi_export]
pub unsafe fn set_global_option(content: GlobalOption) -> GlobalOption {
    let reenter = IN_SET_GLOBAL_OPTION.load(Ordering::SeqCst);
    if reenter {
        return set_global_option_impl(&content);
    }
    IN_SET_GLOBAL_OPTION.store(true, Ordering::SeqCst);

    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_set_global_option_chain(&mut buf) };

    if !interrupted {
        interrupted = unsafe { run_on_set_global_option_chain(&buf) };
    }

    let result = if !interrupted {
        let res = set_global_option_impl(&buf);
        unsafe { run_after_set_global_option_chain(&buf) };
        res
    } else {
        GlobalOption::new("", "")
    };

    IN_SET_GLOBAL_OPTION.store(false, Ordering::SeqCst);
    result
}

// ===== exists_global_option 钩子系统 =====
type BeforeExistsGlobalOptionHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnExistsGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterExistsGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_EXISTS_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<BeforeExistsGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_EXISTS_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<OnExistsGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_EXISTS_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<AfterExistsGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_EXISTS_GLOBAL_OPTION: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_exists_global_option(hook: BeforeExistsGlobalOptionHook) {
    if let Ok(mut hooks) = BEFORE_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_exists_global_option(hook: OnExistsGlobalOptionHook) {
    if let Ok(mut hooks) = ON_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_exists_global_option(hook: AfterExistsGlobalOptionHook) {
    if let Ok(mut hooks) = AFTER_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_exists_global_option_hooks() {
    let _ = BEFORE_EXISTS_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_EXISTS_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_EXISTS_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_exists_global_option_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_on_exists_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_after_exists_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_EXISTS_GLOBAL_OPTION_HOOKS.lock() {
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

fn exists_global_option_impl(key: &repr_c::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**key)
}

#[ffi_export]
pub unsafe fn exists_global_option(content: repr_c::String) -> bool {
    let reenter = IN_EXISTS_GLOBAL_OPTION.load(Ordering::SeqCst);
    if reenter {
        return exists_global_option_impl(&content);
    }
    IN_EXISTS_GLOBAL_OPTION.store(true, Ordering::SeqCst);

    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_exists_global_option_chain(&mut buf) };

    if !interrupted {
        interrupted = unsafe { run_on_exists_global_option_chain(&buf) };
    }

    let result = if !interrupted {
        let res = exists_global_option_impl(&buf);
        unsafe { run_after_exists_global_option_chain(&buf) };
        res
    } else {
        false
    };

    IN_EXISTS_GLOBAL_OPTION.store(false, Ordering::SeqCst);
    result
}

// ===== get_global_option 钩子系统 =====
type BeforeGetGlobalOptionHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnGetGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterGetGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_GET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<BeforeGetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_GET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<OnGetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_GET_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<AfterGetGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_GET_GLOBAL_OPTION: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_get_global_option(hook: BeforeGetGlobalOptionHook) {
    if let Ok(mut hooks) = BEFORE_GET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_get_global_option(hook: OnGetGlobalOptionHook) {
    if let Ok(mut hooks) = ON_GET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_get_global_option(hook: AfterGetGlobalOptionHook) {
    if let Ok(mut hooks) = AFTER_GET_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_get_global_option_hooks() {
    let _ = BEFORE_GET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_GET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_GET_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_get_global_option_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_GET_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_on_get_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_GET_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_after_get_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_GET_GLOBAL_OPTION_HOOKS.lock() {
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

fn get_global_option_impl(key: &repr_c::String) -> TaggedOption<repr_c::String> {
    let map = MAP.lock().unwrap();
    map.get(&**key).cloned().map(|v| v.into()).into()
}

#[ffi_export]
pub unsafe fn get_global_option(content: repr_c::String) -> TaggedOption<repr_c::String> {
    let reenter = IN_GET_GLOBAL_OPTION.load(Ordering::SeqCst);
    if reenter {
        return get_global_option_impl(&content);
    }
    IN_GET_GLOBAL_OPTION.store(true, Ordering::SeqCst);

    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_get_global_option_chain(&mut buf) };

    if !interrupted {
        interrupted = unsafe { run_on_get_global_option_chain(&buf) };
    }

    let result = if !interrupted {
        let res = get_global_option_impl(&buf);
        unsafe { run_after_get_global_option_chain(&buf) };
        res
    } else {
        TaggedOption::None
    };

    IN_GET_GLOBAL_OPTION.store(false, Ordering::SeqCst);
    result
}

// ===== remove_global_option 钩子系统 =====
type BeforeRemoveGlobalOptionHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnRemoveGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterRemoveGlobalOptionHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_REMOVE_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<BeforeRemoveGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_REMOVE_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<OnRemoveGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_REMOVE_GLOBAL_OPTION_HOOKS: LazyLock<Mutex<Vec<AfterRemoveGlobalOptionHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_REMOVE_GLOBAL_OPTION: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_remove_global_option(hook: BeforeRemoveGlobalOptionHook) {
    if let Ok(mut hooks) = BEFORE_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_remove_global_option(hook: OnRemoveGlobalOptionHook) {
    if let Ok(mut hooks) = ON_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_remove_global_option(hook: AfterRemoveGlobalOptionHook) {
    if let Ok(mut hooks) = AFTER_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_remove_global_option_hooks() {
    let _ = BEFORE_REMOVE_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_REMOVE_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_REMOVE_GLOBAL_OPTION_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_remove_global_option_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_on_remove_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
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

unsafe fn run_after_remove_global_option_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_REMOVE_GLOBAL_OPTION_HOOKS.lock() {
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

fn remove_global_option_impl(key: &repr_c::String) -> TaggedOption<repr_c::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**key).map(|v| v.into()).into()
}

#[ffi_export]
pub unsafe fn remove_global_option(content: repr_c::String) -> TaggedOption<repr_c::String> {
    let reenter = IN_REMOVE_GLOBAL_OPTION.load(Ordering::SeqCst);
    if reenter {
        return remove_global_option_impl(&content);
    }
    IN_REMOVE_GLOBAL_OPTION.store(true, Ordering::SeqCst);

    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_remove_global_option_chain(&mut buf) };

    if !interrupted {
        interrupted = unsafe { run_on_remove_global_option_chain(&buf) };
    }

    let result = if !interrupted {
        let res = remove_global_option_impl(&buf);
        unsafe { run_after_remove_global_option_chain(&buf) };
        res
    } else {
        TaggedOption::None
    };

    IN_REMOVE_GLOBAL_OPTION.store(false, Ordering::SeqCst);
    result
}

// ===== get_all_options 钩子系统 =====
type BeforeGetAllOptionsHook = unsafe extern "C" fn() -> bool;
type OnGetAllOptionsHook = unsafe extern "C" fn() -> bool;
type AfterGetAllOptionsHook = unsafe extern "C" fn() -> bool;

static BEFORE_GET_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<BeforeGetAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_GET_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<OnGetAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_GET_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<AfterGetAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_GET_ALL_OPTIONS: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_get_all_options(hook: BeforeGetAllOptionsHook) {
    if let Ok(mut hooks) = BEFORE_GET_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_get_all_options(hook: OnGetAllOptionsHook) {
    if let Ok(mut hooks) = ON_GET_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_get_all_options(hook: AfterGetAllOptionsHook) {
    if let Ok(mut hooks) = AFTER_GET_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_get_all_options_hooks() {
    let _ = BEFORE_GET_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_GET_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_GET_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_get_all_options_chain() -> bool {
    if let Ok(list) = BEFORE_GET_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_get_all_options_chain() -> bool {
    if let Ok(list) = ON_GET_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_get_all_options_chain() -> bool {
    if let Ok(list) = AFTER_GET_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

fn get_all_options_impl() -> safer_ffi::vec::Vec<GlobalOption> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| GlobalOption::new(k, v)).collect::<Vec<_>>().into()
}

#[ffi_export]
pub unsafe fn get_all_options() -> safer_ffi::vec::Vec<GlobalOption> {
    let reenter = IN_GET_ALL_OPTIONS.load(Ordering::SeqCst);
    if reenter {
        return get_all_options_impl();
    }
    IN_GET_ALL_OPTIONS.store(true, Ordering::SeqCst);

    let mut interrupted = unsafe { run_before_get_all_options_chain() };

    if !interrupted {
        interrupted = unsafe { run_on_get_all_options_chain() };
    }

    let result = if !interrupted {
        let res = get_all_options_impl();
        unsafe { run_after_get_all_options_chain() };
        res
    } else {
        safer_ffi::vec::Vec::from(Vec::new())
    };

    IN_GET_ALL_OPTIONS.store(false, Ordering::SeqCst);
    result
}

// ===== clear_all_options 钩子系统 =====
type BeforeClearAllOptionsHook = unsafe extern "C" fn() -> bool;
type OnClearAllOptionsHook = unsafe extern "C" fn() -> bool;
type AfterClearAllOptionsHook = unsafe extern "C" fn() -> bool;

static BEFORE_CLEAR_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<BeforeClearAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_CLEAR_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<OnClearAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_CLEAR_ALL_OPTIONS_HOOKS: LazyLock<Mutex<Vec<AfterClearAllOptionsHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_CLEAR_ALL_OPTIONS: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_clear_all_options(hook: BeforeClearAllOptionsHook) {
    if let Ok(mut hooks) = BEFORE_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_clear_all_options(hook: OnClearAllOptionsHook) {
    if let Ok(mut hooks) = ON_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_clear_all_options(hook: AfterClearAllOptionsHook) {
    if let Ok(mut hooks) = AFTER_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_clear_all_options_hooks() {
    let _ = BEFORE_CLEAR_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_CLEAR_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_CLEAR_ALL_OPTIONS_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_clear_all_options_chain() -> bool {
    if let Ok(list) = BEFORE_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_clear_all_options_chain() -> bool {
    if let Ok(list) = ON_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_clear_all_options_chain() -> bool {
    if let Ok(list) = AFTER_CLEAR_ALL_OPTIONS_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h() {
                    return true;
                }
            }
        }
    }
    false
}

fn clear_all_options_impl() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}

#[ffi_export]
pub unsafe fn clear_all_options() {
    let reenter = IN_CLEAR_ALL_OPTIONS.load(Ordering::SeqCst);
    if reenter {
        clear_all_options_impl();
        return;
    }
    IN_CLEAR_ALL_OPTIONS.store(true, Ordering::SeqCst);

    let interrupted = unsafe { run_before_clear_all_options_chain() };

    if !interrupted {
        let interrupted = unsafe { run_on_clear_all_options_chain() };
        if !interrupted {
            clear_all_options_impl();
            unsafe { run_after_clear_all_options_chain() };
        }
    }

    IN_CLEAR_ALL_OPTIONS.store(false, Ordering::SeqCst);
}
