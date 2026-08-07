use crate::Void;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: String,
    pub code: String,
}

impl MacroDef {
    pub fn new(name: &str, code: &str) -> Self {
        MacroDef { name: name.to_string(), code: code.to_string() }
    }
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn exists_macro(name: &str) -> bool {
    let map = MAP.lock().unwrap();
    map.get(name).is_some()
}

fn internal_set_macro(r#macro: &MacroDef) {
    let mut map = MAP.lock().unwrap();
    map.insert(r#macro.name.clone(), r#macro.code.clone());
}

fn internal_remove_macro(name: &str) -> Option<String> {
    let mut map = MAP.lock().unwrap();
    map.remove(name)
}

fn internal_get_macro(name: &str) -> Option<String> {
    let map = MAP.lock().unwrap();
    map.get(name).cloned()
}

fn internal_clear_all_macros(_v: Void) {
    let mut map = MAP.lock().unwrap();
    map.clear();
}

crate::define_hook_system!(
    internal_set_macro,
    "set_macro",
    M,
    R,
    R,
    R,
    &mut MacroDef,
    &MacroDef,
    &MacroDef,
    MacroDef,
    ()
);

crate::define_hook_system!(
    internal_clear_all_macros,
    "clear_all_macros",
    V,
    V,
    V,
    V,
    Void,
    Void,
    Void,
    Void,
    ()
);

crate::define_hook_system!(
    internal_get_macro,
    "get_macro",
    M,
    R,
    R,
    R,
    &mut String,
    &String,
    &String,
    String,
    Option<String>
);

crate::define_hook_system!(
    internal_remove_macro,
    "remove_macro",
    M,
    R,
    R,
    R,
    &mut String,
    &String,
    &String,
    String,
    Option<String>
);
