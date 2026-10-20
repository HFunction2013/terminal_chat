//! 命令别名（alias）存储。
//!
//! 别名与宏（`macros.rs`）是两套机制：宏把多行命令当脚本执行；
//! 别名则把第一个 token 替换为目标命令（可含参数），
//! 在 `run_command` 入口统一展开。

use ::safer_ffi::option::TaggedOption;
use ::safer_ffi::prelude::*;
use cli_core_types::AliasDef;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 设置一个别名（alias 名 → 目标命令）。
#[register_hook]
fn set_alias(alias: &AliasDef) -> AliasDef {
    let mut map = MAP.lock().unwrap();
    map.insert(alias.name.to_string(), alias.target.to_string());
    alias.clone()
}

/// 别名是否存在。
#[register_hook]
fn exists_alias(name: &safer_ffi::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**name)
}

/// 取别名的目标命令。
#[register_hook(fallback = "TaggedOption::None")]
fn get_alias(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let map = MAP.lock().unwrap();
    map.get(&**name).cloned().map(|v| v.into()).into()
}

/// 删除别名。
#[register_hook(fallback = "TaggedOption::None")]
fn remove_alias(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let mut map = MAP.lock().unwrap();
    map.remove(&**name).map(|v| v.into()).into()
}

/// 取全部别名。
#[register_hook(fallback = "safer_ffi::vec::Vec::from(Vec::new())")]
fn get_all_aliases() -> safer_ffi::vec::Vec<AliasDef> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| AliasDef::new(k, v)).collect::<Vec<_>>().into()
}

/// 清空所有别名。
#[register_hook]
fn clear_all_aliases() {
    let mut map = MAP.lock().unwrap();
    map.clear();
}
