//! 底部状态栏整行回调登记（#90，批次 4b）。
//!
//! 插件登记一个逐帧底部回调：读取选项中的用户名 / 服务器 / 端口 / 静音态，经
//! `get_current_freq` 读取当前频点，拼成与原内置状态栏一致的整行文本返回宿主。
//! 宿主只渲染回调文本；过渡期无回调时回退内置硬编码整行（见 #150）。

use crate::LIB;
use libloading::Library;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;
use std::time::{SystemTime, UNIX_EPOCH};

/// 全局选项真值判定（对齐宿主 `is_truthy`）。
fn is_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_lowercase().as_str(),
        "on" | "true" | "1" | "yes" | "y" | "enabled" | "enable"
    )
}

/// 经 coreapi 读取一个全局选项；未设置或调用失败返回 `None`。
fn read_global(key: &str) -> Option<String> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, get_global_option);
    let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
        ManuallyDrop::new(TaggedOption::None);
    let _res = get_global_option(&key.into(), Out::from(&mut buf));
    match ManuallyDrop::into_inner(buf) {
        TaggedOption::Some(v) => Some(v.to_string()),
        TaggedOption::None => None,
    }
}

/// 读取当前频点；未加入频点时返回 `None`。
fn read_freq() -> Option<String> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, get_current_freq);
    let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
        ManuallyDrop::new(TaggedOption::None);
    let _res = get_current_freq(Out::from(&mut buf));
    match ManuallyDrop::into_inner(buf) {
        TaggedOption::Some(v) => Some(v.to_string()),
        TaggedOption::None => None,
    }
}

/// 拼接待渲染的状态栏整行（纯函数，便于单测覆盖空值回退口径）。
fn compose_status_line(
    username: &str,
    server: &str,
    port: &str,
    quiet: bool,
    freq: &str,
    ts: u64,
) -> String {
    let who = if username.is_empty() { "guest" } else { username };
    let host = if server.is_empty() {
        "offline".to_string()
    } else if port.is_empty() {
        server.to_string()
    } else {
        format!("{server}:{port}")
    };
    let quiet_label = if quiet { "QUIET" } else { "sound" };
    format!("{who}@{host} | freq {freq} | {quiet_label} | {ts}")
}

/// 逐帧底部回调：取当前状态拼成整行文本；空串表示本帧无片段。
pub unsafe extern "C" fn status_bar_callback() -> safer_ffi::String {
    let username = read_global("USERNAME").unwrap_or_default();
    let server = read_global("SERVER").unwrap_or_default();
    let port = read_global("PORT").unwrap_or_default();
    let quiet = is_truthy(&read_global("QUIET").unwrap_or_default());
    let freq = read_freq().unwrap_or_else(|| "-".to_string());
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    compose_status_line(&username, &server, &port, quiet, &freq, ts).into()
}

/// 向 cli-core 注册表登记底部整行回调。
pub fn register_statusbar(lib: &Library) {
    cli_core_macros::load_core_symbols!(lib, register_bottom_callback);
    let cb: cli_core_types::ui::BottomFragmentFn = status_bar_callback;
    let _ = register_bottom_callback(&cb);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_line_falls_back_to_defaults() {
        // 全空：guest@offline | freq - | sound | 0
        let line = compose_status_line("", "", "", false, "-", 0);
        assert_eq!(line, "guest@offline | freq - | sound | 0");
    }

    #[test]
    fn compose_line_renders_host_and_quiet() {
        let line = compose_status_line("alice", "127.0.0.1", "50051", true, "lobby", 1700);
        assert_eq!(line, "alice@127.0.0.1:50051 | freq lobby | QUIET | 1700");
    }

    #[test]
    fn compose_line_omits_port_when_empty() {
        let line = compose_status_line("bob", "example.com", "", false, "-", 1);
        assert_eq!(line, "bob@example.com | freq - | sound | 1");
    }
}
