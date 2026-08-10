use ::safer_ffi::prelude::*;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: repr_c::String,
    pub code: repr_c::String,
}

impl MacroDef {
    pub fn new(name: &str, code: &str) -> Self {
        MacroDef { 
            name: name.into(), 
            code: code.into() 
        }
    }
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ===== set_macro 钩子系统 =====
type BeforeSetMacroHook = unsafe extern "C" fn(*mut MacroDef) -> bool;
type OnSetMacroHook = unsafe extern "C" fn(*const MacroDef) -> bool;
type AfterSetMacroHook = unsafe extern "C" fn(*const MacroDef) -> bool;

static BEFORE_SET_MACRO_HOOKS: LazyLock<Mutex<Vec<BeforeSetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_SET_MACRO_HOOKS: LazyLock<Mutex<Vec<OnSetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_SET_MACRO_HOOKS: LazyLock<Mutex<Vec<AfterSetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_SET_MACRO: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_set_macro(hook: BeforeSetMacroHook) {
    if let Ok(mut hooks) = BEFORE_SET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_set_macro(hook: OnSetMacroHook) {
    if let Ok(mut hooks) = ON_SET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_set_macro(hook: AfterSetMacroHook) {
    if let Ok(mut hooks) = AFTER_SET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_set_macro_hooks() {
    let _ = BEFORE_SET_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_SET_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_SET_MACRO_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_set_macro_chain(buf: &mut MacroDef) -> bool {
    if let Ok(list) = BEFORE_SET_MACRO_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *mut MacroDef) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_set_macro_chain(buf: &MacroDef) -> bool {
    if let Ok(list) = ON_SET_MACRO_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const MacroDef) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_set_macro_chain(buf: &MacroDef) -> bool {
    if let Ok(list) = AFTER_SET_MACRO_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const MacroDef) {
                    return true;
                }
            }
        }
    }
    false
}

fn set_macro_impl(r#macro: &MacroDef) -> MacroDef {
    let mut map = MAP.lock().unwrap();
    map.insert(r#macro.name.to_string(), r#macro.code.to_string());
    r#macro.clone()
}

#[ffi_export]
pub unsafe fn set_macro(content: MacroDef) -> MacroDef {
    let reenter = IN_SET_MACRO.load(Ordering::SeqCst);
    if reenter {
        return set_macro_impl(&content);
    }
    IN_SET_MACRO.store(true, Ordering::SeqCst);
    
    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_set_macro_chain(&mut buf) };
    
    if !interrupted {
        interrupted = unsafe { run_on_set_macro_chain(&buf) };
    }
    
    let result = if !interrupted {
        let res = set_macro_impl(&buf);
        unsafe { run_after_set_macro_chain(&buf) };
        res
    } else {
        MacroDef::new("", "")
    };
    
    IN_SET_MACRO.store(false, Ordering::SeqCst);
    result
}

// ===== exists_macro 钩子系统 =====
type BeforeExistsMacroHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnExistsMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterExistsMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_EXISTS_MACRO_HOOKS: LazyLock<Mutex<Vec<BeforeExistsMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_EXISTS_MACRO_HOOKS: LazyLock<Mutex<Vec<OnExistsMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_EXISTS_MACRO_HOOKS: LazyLock<Mutex<Vec<AfterExistsMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_EXISTS_MACRO: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_exists_macro(hook: BeforeExistsMacroHook) {
    if let Ok(mut hooks) = BEFORE_EXISTS_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_exists_macro(hook: OnExistsMacroHook) {
    if let Ok(mut hooks) = ON_EXISTS_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_exists_macro(hook: AfterExistsMacroHook) {
    if let Ok(mut hooks) = AFTER_EXISTS_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_exists_macro_hooks() {
    let _ = BEFORE_EXISTS_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_EXISTS_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_EXISTS_MACRO_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_exists_macro_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_EXISTS_MACRO_HOOKS.lock() {
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

unsafe fn run_on_exists_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_EXISTS_MACRO_HOOKS.lock() {
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

unsafe fn run_after_exists_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_EXISTS_MACRO_HOOKS.lock() {
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

fn exists_macro_impl(name: &repr_c::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**name)
}

#[ffi_export]
pub unsafe fn exists_macro(content: repr_c::String) -> bool {
    let reenter = IN_EXISTS_MACRO.load(Ordering::SeqCst);
    if reenter {
        return exists_macro_impl(&content);
    }
    IN_EXISTS_MACRO.store(true, Ordering::SeqCst);
    
    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_exists_macro_chain(&mut buf) };
    
    if !interrupted {
        interrupted = unsafe { run_on_exists_macro_chain(&buf) };
    }
    
    let result = if !interrupted {
        let res = exists_macro_impl(&buf);
        unsafe { run_after_exists_macro_chain(&buf) };
        res
    } else {
        false
    };
    
    IN_EXISTS_MACRO.store(false, Ordering::SeqCst);
    result
}

// ===== get_macro 钩子系统 =====
type BeforeGetMacroHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnGetMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterGetMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_GET_MACRO_HOOKS: LazyLock<Mutex<Vec<BeforeGetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_GET_MACRO_HOOKS: LazyLock<Mutex<Vec<OnGetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_GET_MACRO_HOOKS: LazyLock<Mutex<Vec<AfterGetMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_GET_MACRO: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_get_macro(hook: BeforeGetMacroHook) {
    if let Ok(mut hooks) = BEFORE_GET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_get_macro(hook: OnGetMacroHook) {
    if let Ok(mut hooks) = ON_GET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_get_macro(hook: AfterGetMacroHook) {
    if let Ok(mut hooks) = AFTER_GET_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_get_macro_hooks() {
    let _ = BEFORE_GET_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_GET_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_GET_MACRO_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_get_macro_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_GET_MACRO_HOOKS.lock() {
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

unsafe fn run_on_get_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_GET_MACRO_HOOKS.lock() {
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

unsafe fn run_after_get_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_GET_MACRO_HOOKS.lock() {
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

fn get_macro_impl(name: &repr_c::String) -> Option<repr_c::String> {
    let map = MAP.lock().unwrap();
    map.get(&**name).cloned().map(|v| v.into())
}

#[ffi_export]
pub unsafe fn get_macro(content: repr_c::String) -> Option<repr_c::String> {
    let reenter = IN_GET_MACRO.load(Ordering::SeqCst);
    if reenter {
        return get_macro_impl(&content);
    }
    IN_GET_MACRO.store(true, Ordering::SeqCst);
    
    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_get_macro_chain(&mut buf) };
    
    if !interrupted {
        interrupted = unsafe { run_on_get_macro_chain(&buf) };
    }
    
    let result = if !interrupted {
        let res = get_macro_impl(&buf);
        unsafe { run_after_get_macro_chain(&buf) };
        res
    } else {
        None
    };
    
    IN_GET_MACRO.store(false, Ordering::SeqCst);
    result
}

// ===== remove_macro 钩子系统 =====
type BeforeRemoveMacroHook = unsafe extern "C" fn(*mut repr_c::String) -> bool;
type OnRemoveMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;
type AfterRemoveMacroHook = unsafe extern "C" fn(*const repr_c::String) -> bool;

static BEFORE_REMOVE_MACRO_HOOKS: LazyLock<Mutex<Vec<BeforeRemoveMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_REMOVE_MACRO_HOOKS: LazyLock<Mutex<Vec<OnRemoveMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_REMOVE_MACRO_HOOKS: LazyLock<Mutex<Vec<AfterRemoveMacroHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_REMOVE_MACRO: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_remove_macro(hook: BeforeRemoveMacroHook) {
    if let Ok(mut hooks) = BEFORE_REMOVE_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_remove_macro(hook: OnRemoveMacroHook) {
    if let Ok(mut hooks) = ON_REMOVE_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_remove_macro(hook: AfterRemoveMacroHook) {
    if let Ok(mut hooks) = AFTER_REMOVE_MACRO_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_remove_macro_hooks() {
    let _ = BEFORE_REMOVE_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_REMOVE_MACRO_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_REMOVE_MACRO_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_remove_macro_chain(buf: &mut repr_c::String) -> bool {
    if let Ok(list) = BEFORE_REMOVE_MACRO_HOOKS.lock() {
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

unsafe fn run_on_remove_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = ON_REMOVE_MACRO_HOOKS.lock() {
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

unsafe fn run_after_remove_macro_chain(buf: &repr_c::String) -> bool {
    if let Ok(list) = AFTER_REMOVE_MACRO_HOOKS.lock() {
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

fn remove_macro_impl(name: &repr_c::String) -> Option<repr_c::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**name).map(|v| v.into())
}

#[ffi_export]
pub unsafe fn remove_macro(content: repr_c::String) -> Option<repr_c::String> {
    let reenter = IN_REMOVE_MACRO.load(Ordering::SeqCst);
    if reenter {
        return remove_macro_impl(&content);
    }
    IN_REMOVE_MACRO.store(true, Ordering::SeqCst);
    
    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_remove_macro_chain(&mut buf) };
    
    if !interrupted {
        interrupted = unsafe { run_on_remove_macro_chain(&buf) };
    }
    
    let result = if !interrupted {
        let res = remove_macro_impl(&buf);
        unsafe { run_after_remove_macro_chain(&buf) };
        res
    } else {
        None
    };
    
    IN_REMOVE_MACRO.store(false, Ordering::SeqCst);
    result
}

// ===== get_all_macros 钩子系统 =====
type BeforeGetAllMacrosHook = unsafe extern "C" fn() -> bool;
type OnGetAllMacrosHook = unsafe extern "C" fn() -> bool;
type AfterGetAllMacrosHook = unsafe extern "C" fn() -> bool;

static BEFORE_GET_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<BeforeGetAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_GET_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<OnGetAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_GET_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<AfterGetAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_GET_ALL_MACROS: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_get_all_macros(hook: BeforeGetAllMacrosHook) {
    if let Ok(mut hooks) = BEFORE_GET_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_get_all_macros(hook: OnGetAllMacrosHook) {
    if let Ok(mut hooks) = ON_GET_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_get_all_macros(hook: AfterGetAllMacrosHook) {
    if let Ok(mut hooks) = AFTER_GET_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_get_all_macros_hooks() {
    let _ = BEFORE_GET_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_GET_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_GET_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_get_all_macros_chain() -> bool {
    if let Ok(list) = BEFORE_GET_ALL_MACROS_HOOKS.lock() {
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

unsafe fn run_on_get_all_macros_chain() -> bool {
    if let Ok(list) = ON_GET_ALL_MACROS_HOOKS.lock() {
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

unsafe fn run_after_get_all_macros_chain() -> bool {
    if let Ok(list) = AFTER_GET_ALL_MACROS_HOOKS.lock() {
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

fn get_all_macros_impl() -> safer_ffi::vec::Vec<MacroDef> {
    let map = MAP.lock().unwrap();
    map.iter()
        .map(|(k, v)| MacroDef::new(k, v))
        .collect::<Vec<_>>()
        .into()
}

#[ffi_export]
pub unsafe fn get_all_macros() -> safer_ffi::vec::Vec<MacroDef> {
    let reenter = IN_GET_ALL_MACROS.load(Ordering::SeqCst);
    if reenter {
        return get_all_macros_impl();
    }
    IN_GET_ALL_MACROS.store(true, Ordering::SeqCst);
    
    let mut interrupted = unsafe { run_before_get_all_macros_chain() };
    
    if !interrupted {
        interrupted = unsafe { run_on_get_all_macros_chain() };
    }
    
    let result = if !interrupted {
        let res = get_all_macros_impl();
        unsafe { run_after_get_all_macros_chain() };
        res
    } else {
        safer_ffi::vec::Vec::from(Vec::new())
    };
    
    IN_GET_ALL_MACROS.store(false, Ordering::SeqCst);
    result
}

// ===== clear_all_macros 钩子系统 =====
type BeforeClearAllMacrosHook = unsafe extern "C" fn() -> bool;
type OnClearAllMacrosHook = unsafe extern "C" fn() -> bool;
type AfterClearAllMacrosHook = unsafe extern "C" fn() -> bool;

static BEFORE_CLEAR_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<BeforeClearAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_CLEAR_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<OnClearAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_CLEAR_ALL_MACROS_HOOKS: LazyLock<Mutex<Vec<AfterClearAllMacrosHook>>> = 
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_CLEAR_ALL_MACROS: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_clear_all_macros(hook: BeforeClearAllMacrosHook) {
    if let Ok(mut hooks) = BEFORE_CLEAR_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_clear_all_macros(hook: OnClearAllMacrosHook) {
    if let Ok(mut hooks) = ON_CLEAR_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_clear_all_macros(hook: AfterClearAllMacrosHook) {
    if let Ok(mut hooks) = AFTER_CLEAR_ALL_MACROS_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_clear_all_macros_hooks() {
    let _ = BEFORE_CLEAR_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_CLEAR_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_CLEAR_ALL_MACROS_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_clear_all_macros_chain() -> bool {
    if let Ok(list) = BEFORE_CLEAR_ALL_MACROS_HOOKS.lock() {
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

unsafe fn run_on_clear_all_macros_chain() -> bool {
    if let Ok(list) = ON_CLEAR_ALL_MACROS_HOOKS.lock() {
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

unsafe fn run_after_clear_all_macros_chain() -> bool {
    if let Ok(list) = AFTER_CLEAR_ALL_MACROS_HOOKS.lock() {
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

fn clear_all_macros_impl() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}

#[ffi_export]
pub unsafe fn clear_all_macros() {
    let reenter = IN_CLEAR_ALL_MACROS.load(Ordering::SeqCst);
    if reenter {
        clear_all_macros_impl();
        return;
    }
    IN_CLEAR_ALL_MACROS.store(true, Ordering::SeqCst);
    
    let interrupted = unsafe { run_before_clear_all_macros_chain() };
    
    if !interrupted {
        let interrupted = unsafe { run_on_clear_all_macros_chain() };
        if !interrupted {
            clear_all_macros_impl();
            unsafe { run_after_clear_all_macros_chain() };
        }
    }
    
    IN_CLEAR_ALL_MACROS.store(false, Ordering::SeqCst);
}