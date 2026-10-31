//! 客户端异步推送队列（宿主侧单实例）。
//!
//! 推送帧由 cli-standard 插件的后台读线程解析后经 `tc_cli_push_enqueue` 写入这里，
//! cli 主程序的后台线程经 `tc_cli_push_drain` 取走并在输入行上方显示。
//! 队列放在宿主（cli_core）是因为 cli-standard 可能被多次加载（cli_core 一次 +
//! cli 主程序自身一次），插件内的 static 不跨加载实例共享；而 cli_core 由
//! cli 主程序加载一次、插件通过宿主句柄访问，天然单实例。

use safer_ffi::ffi_export;
use safer_ffi::option::TaggedOption;
use std::sync::{LazyLock, Mutex};

static PUSH_QUEUE: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 入队一条推送文本（cli-standard 调用）。
#[ffi_export]
pub fn tc_cli_push_enqueue(text: &safer_ffi::String) {
    PUSH_QUEUE.lock().unwrap().push(text.to_string());
}

/// 取走并清空推送队列；`QUIET` 开启时静音（返回空）。
#[ffi_export]
pub fn tc_cli_push_drain() -> safer_ffi::Vec<safer_ffi::String> {
    let mut q = PUSH_QUEUE.lock().unwrap();
    let items: Vec<String> = std::mem::take(&mut *q);
    if items.is_empty() {
        return Vec::new().into();
    }
    // `setg QUIET on/true/1/yes` 时静音全部通知。
    if let TaggedOption::Some(v) = unsafe { crate::global_settings::get_global_option(&"QUIET".into()) } {
        let qv = v.to_string().to_lowercase();
        if matches!(qv.as_str(), "on" | "true" | "1" | "yes") {
            return Vec::new().into();
        }
    }
    items.into_iter().map(|s| s.into()).collect::<Vec<_>>().into()
}
