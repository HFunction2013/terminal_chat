//! 命令别名（alias）存储。
//!
//! 别名与宏（`macros.rs`）是两套机制：宏把多行命令当脚本执行；
//! 别名则把第一个 token 替换为目标命令（可含参数），
//! 在 `run_command` 入口统一展开。
//!
//! 持久化：按当前 workspace 存在 `~/.terminal_chat[/workspaces/<name>]/aliases.txt`，
//! 行格式 `name<TAB>base64(target)`；`reload_aliases` 在切换 workspace 后重新加载。

use ::safer_ffi::option::TaggedOption;
use ::safer_ffi::prelude::*;
use base64::Engine as _;
use cli_core_types::AliasDef;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 当前 workspace 的状态根（与 cli-standard 的 `local::state_dir` 规则一致）。
fn state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let base = PathBuf::from(home).join(".terminal_chat");
    let cur = std::fs::read_to_string(base.join("current_workspace"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    if cur == "default" {
        base
    } else {
        base.join("workspaces").join(cur)
    }
}

fn alias_file() -> PathBuf {
    state_dir().join("aliases.txt")
}

fn load_into(map: &mut HashMap<String, String>) {
    let Ok(content) = std::fs::read_to_string(alias_file()) else {
        return;
    };
    for line in content.lines() {
        let Some((name, encoded)) = line.split_once('\t') else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) {
            if let Ok(target) = String::from_utf8(bytes) {
                map.insert(name.to_string(), target);
            }
        }
    }
}

fn save(map: &HashMap<String, String>) {
    let mut content = String::new();
    for (k, v) in map {
        content.push_str(&format!(
            "{k}\t{}\n",
            base64::engine::general_purpose::STANDARD.encode(v)
        ));
    }
    let _ = std::fs::write(alias_file(), content);
}

static MAP: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    load_into(&mut m);
    Mutex::new(m)
});

/// 设置一个别名（alias 名 → 目标命令）。
#[register_hook]
fn set_alias(alias: &AliasDef) -> AliasDef {
    let mut map = MAP.lock().unwrap();
    map.insert(alias.name.to_string(), alias.target.to_string());
    save(&map);
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
    let old = map.remove(&**name).map(|v| v.into());
    save(&map);
    old.into()
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
    save(&map);
}

/// 重新从当前 workspace 的别名文件加载（切换 workspace 后调用）。
#[register_hook]
fn reload_aliases() {
    let mut map = MAP.lock().unwrap();
    map.clear();
    load_into(&mut map);
}
