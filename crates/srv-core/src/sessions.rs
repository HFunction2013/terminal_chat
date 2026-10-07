//! 客户端会话（session）骨架。
//!
//! 会话代表一个已连接的客户端。当前只提供注册表骨架，
//! 真实连接的建立 / 断开由后续传输层驱动（见 `sessions.rs` 类型）。

use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};

use srv_core_types::sessions::SessionInfo;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// 会话表：会话 id → 会话信息。
static SESSION_MAP: LazyLock<Mutex<HashMap<String, SessionInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 添加一个会话（id 已存在则失败）。
#[register_hook]
pub fn add_session(session: &SessionInfo) -> i32 {
    let mut map = SESSION_MAP.lock().unwrap();
    let key = session.id.to_string();
    if map.contains_key(&key) {
        false.into()
    } else {
        map.insert(key, session.clone());
        true.into()
    }
}

/// 移除一个会话。
#[register_hook]
pub fn remove_session(id: &safer_ffi::String) -> i32 {
    let mut map = SESSION_MAP.lock().unwrap();
    map.remove(&id.to_string()).map(|_| true).unwrap_or(false).into()
}

/// 会话是否存在。
#[register_hook]
pub fn has_session(id: &safer_ffi::String) -> bool {
    let map = SESSION_MAP.lock().unwrap();
    map.contains_key(&id.to_string())
}

/// 当前会话数量。
#[register_hook]
pub fn session_count() -> i32 {
    let map = SESSION_MAP.lock().unwrap();
    map.len() as i32
}

/// 清空所有会话。
#[register_hook]
pub fn clear_sessions() {
    let mut map = SESSION_MAP.lock().unwrap();
    map.clear();
}

/// 获取全部会话信息。
#[register_hook(fallback = "safer_ffi::Vec::from(std::vec::Vec::new())")]
pub fn get_all_sessions() -> safer_ffi::Vec<SessionInfo> {
    let map = SESSION_MAP.lock().unwrap();
    let sessions: Vec<SessionInfo> = map.values().cloned().collect();
    sessions.into()
}
