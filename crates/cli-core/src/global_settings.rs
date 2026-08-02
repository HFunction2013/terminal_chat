use crate::Void;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone)]
pub struct GlobalOption {
    pub key: String,
    pub value: String,
}

impl GlobalOption {
    pub fn new(key: &str, value: &str) -> Self {
        GlobalOption { key: key.to_string(), value: value.to_string() }
    }
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn exists_global_option(key: &str) -> bool {
    let map = MAP.lock().unwrap();
    map.get(key).is_some()
}

fn internal_set_global_option(option: &GlobalOption) {
    let mut map = MAP.lock().unwrap();
    map.insert(option.key.clone(), option.value.clone());
}

fn internal_remove_global_option(key: &str) -> Option<String> {
    let mut map = MAP.lock().unwrap();
    map.remove(key)
}

fn internal_get_global_option(key: &str) -> Option<String> {
    let map = MAP.lock().unwrap();
    map.get(key).cloned()
}

fn internal_get_all_options(_v: Void) -> HashMap<String, String> {
    let map = MAP.lock().unwrap();
    map.clone()
}

fn internal_clear_all_options(_v: Void) {
    let mut map = MAP.lock().unwrap();
    map.clear();
}
// set_global_option 返回 ()
crate::define_hook_system!(
    internal_set_global_option,
    "set_global_option",
    M,
    R,
    R,
    R,
    &mut GlobalOption,
    &GlobalOption,
    &GlobalOption,
    GlobalOption,
    () // 返回值类型
);

// clear_all_options 返回 ()
crate::define_hook_system!(
    internal_clear_all_options,
    "clear_all_options",
    V,
    V,
    V,
    V,
    Void,
    Void,
    Void,
    Void,
    () // 返回值类型
);

// get_all_options 返回 HashMap<String, String>
crate::define_hook_system!(
    internal_get_all_options,
    "get_all_options",
    V, V, V, V,
    Void, Void, Void, Void,
    HashMap<String, String>  // 返回值类型
);

// get_global_option 返回 Option<String>
crate::define_hook_system!(
    internal_get_global_option,
    "get_global_option",
    M,
    R,
    R,
    R,
    &mut String,
    &String,
    &String,
    String,
    Option<String> // 返回值类型
);

// remove_global_option 返回 Option<String>
crate::define_hook_system!(
    internal_remove_global_option,
    "remove_global_option",
    M,
    R,
    R,
    R,
    &mut String,
    &String,
    &String,
    String,
    Option<String> // 返回值类型
);
