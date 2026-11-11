//! 宏（macro）存储：多行命令按顺序执行。
//!
//! 持久化：按当前 workspace 存在 `~/.terminal_chat[/workspaces/<name>]/macros.txt`，
//! 行格式 `name<TAB>base64(code)`；`reload_macros` 在切换 workspace 后重新加载。

use ::safer_ffi::option::TaggedOption;
use ::safer_ffi::prelude::*;
use base64::Engine as _;
use cli_core_types::MacroDef;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

fn state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let base = PathBuf::from(home).join(".terminal_chat");
    let cur = std::fs::read_to_string(base.join("current_workspace"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    if cur == "default" { base } else { base.join("workspaces").join(cur) }
}

fn macro_file() -> PathBuf {
    state_dir().join("macros.txt")
}

fn load_into(map: &mut HashMap<String, String>) {
    let Ok(content) = std::fs::read_to_string(macro_file()) else {
        return;
    };
    for line in content.lines() {
        let Some((name, encoded)) = line.split_once('\t') else {
            continue;
        };
        let name_trimmed = name.trim();
        if name_trimmed.is_empty() {
            continue;
        }
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) {
            if let Ok(code) = String::from_utf8(bytes) {
                map.insert(name_trimmed.to_string(), code);
            }
        }
    }
}

fn save(map: &HashMap<String, String>) {
    let mut content = String::new();
    for (k, v) in map {
        content
            .push_str(&format!("{k}\t{}\n", base64::engine::general_purpose::STANDARD.encode(v)));
    }
    let _ = std::fs::write(macro_file(), content);
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    load_into(&mut m);
    Mutex::new(m)
});

#[register_hook]
fn set_macro(r#macro: &MacroDef) -> MacroDef {
    let mut map = MAP.lock().unwrap();
    map.insert(r#macro.name.to_string(), r#macro.code.to_string());
    save(&map);
    r#macro.clone()
}

#[register_hook]
fn exists_macro(name: &safer_ffi::String) -> bool {
    let map = MAP.lock().unwrap();
    map.contains_key(&**name)
}

#[register_hook(fallback = "TaggedOption::None")]
fn get_macro(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let map = MAP.lock().unwrap();
    map.get(&**name).cloned().map(|v| v.into()).into()
}

#[register_hook(fallback = "TaggedOption::None")]
fn remove_macro(name: &safer_ffi::String) -> TaggedOption<safer_ffi::String> {
    let mut map = MAP.lock().unwrap();
    let old = map.remove(&**name).map(|v| v.into());
    save(&map);
    old.into()
}

#[register_hook(fallback = "safer_ffi::vec::Vec::from(Vec::new())")]
fn get_all_macros() -> safer_ffi::vec::Vec<MacroDef> {
    let map = MAP.lock().unwrap();
    map.iter().map(|(k, v)| MacroDef::new(k, v)).collect::<Vec<_>>().into()
}

#[register_hook]
fn clear_all_macros() {
    let mut map = MAP.lock().unwrap();
    map.clear();
    save(&map);
}

/// 重新从当前 workspace 的宏文件加载（切换 workspace 后调用）。
#[register_hook]
fn reload_macros() {
    let mut map = MAP.lock().unwrap();
    map.clear();
    load_into(&mut map);
}
