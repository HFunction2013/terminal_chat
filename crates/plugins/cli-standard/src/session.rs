//! 客户端会话状态：一个 freq 是一个会话。
//!
//! `join_freq`/`create_freq` 成功后记录为当前会话；`background` 挂起当前
//! freq（服务端 leave_freq + 本地放入后台列表）；`foreground` 恢复最近挂起的
//! freq（重新 join_freq）；`sessions` 列出当前与后台会话。

use std::sync::{LazyLock, Mutex};

static CURRENT_FREQ: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static BACKGROUND_FREQS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 记录当前会话（join/create 成功后调用），并从后台列表移除同名项。
pub fn set_current(freq: &str) {
    *CURRENT_FREQ.lock().unwrap() = Some(freq.to_string());
    BACKGROUND_FREQS.lock().unwrap().retain(|f| f != freq);
}

/// 清空当前会话（leave_freq 成功后调用）。
pub fn clear_current() {
    *CURRENT_FREQ.lock().unwrap() = None;
}

/// 当前会话。
pub fn current() -> Option<String> {
    CURRENT_FREQ.lock().unwrap().clone()
}

/// 挂起当前会话：取走 CURRENT_FREQ 并放入后台列表（background 用）。
pub fn background_current() -> Option<String> {
    let f = CURRENT_FREQ.lock().unwrap().take();
    if let Some(f) = &f {
        BACKGROUND_FREQS.lock().unwrap().push(f.clone());
    }
    f
}

/// 恢复最近挂起的会话（foreground 用）。
pub fn foreground_next() -> Option<String> {
    BACKGROUND_FREQS.lock().unwrap().pop()
}

/// (当前会话, 后台会话列表)。
pub fn all() -> (Option<String>, Vec<String>) {
    (current(), BACKGROUND_FREQS.lock().unwrap().clone())
}
