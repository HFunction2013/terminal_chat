//! 客户端异步推送队列（宿主侧单实例）。
//!
//! 推送帧由 cli-standard 插件的后台读线程解析后经 `tc_cli_push_enqueue` 写入这里，
//! cli 主程序的后台线程经 `tc_cli_push_drain` 取走并在输入行上方显示。
//! 队列放在宿主（cli_core）是因为 cli-standard 可能被多次加载（cli_core 一次 +
//! cli 主程序自身一次），插件内的 static 不跨加载实例共享；而 cli_core 由
//! cli 主程序加载一次、插件通过宿主句柄访问，天然单实例。

use safer_ffi::ffi_export;
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::Duration;

static PUSH_QUEUE: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 队列非空时唤醒等待方；用于把“固定间隔轮询 drain”换成“事件到达即醒”。
static PUSH_NOTIFY: LazyLock<Condvar> = LazyLock::new(Condvar::new);

/// 入队一条推送文本（cli-standard 调用）。
#[ffi_export]
pub fn tc_cli_push_enqueue(text: &safer_ffi::String) {
    let mut q = PUSH_QUEUE.lock().unwrap();
    q.push(text.to_string());
    // 唤醒正在阻塞等待的接收方（若有）；无等待方时 notify_one 为空操作。
    PUSH_NOTIFY.notify_one();
}

/// 阻塞等待直到队列非空或超时。返回 `1` 表示有待取项，`0` 表示超时仍为空。
///
/// 供 cli 后台接收线程替代固定 `sleep` 轮询：事件入队即被唤醒，仅以超时作为
/// 兜底心跳。与 [`tc_cli_push_drain`] 配对：返回 `1` 后再 drain 取走文本。
#[ffi_export]
pub fn tc_cli_push_wait(timeout_ms: u32) -> i32 {
    let q = PUSH_QUEUE.lock().unwrap();
    let (guard, timed_out) =
        PUSH_NOTIFY.wait_timeout(q, Duration::from_millis(timeout_ms.max(1) as u64)).unwrap();
    if timed_out.timed_out() && guard.is_empty() { 0 } else { 1 }
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
    if crate::global_settings::quiet_enabled() {
        return Vec::new().into();
    }
    items.into_iter().map(|s| s.into()).collect::<Vec<_>>().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 串行化对全局队列的测试，避免并行用例互相污染。
    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn wait_wakes_on_enqueue() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _ = tc_cli_push_drain();

        let h = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            tc_cli_push_enqueue(&"hello".into());
        });
        // 后台入队应唤醒等待方，而不是等到 1s 超时。
        let started = std::time::Instant::now();
        assert_eq!(tc_cli_push_wait(1000), 1);
        assert!(started.elapsed() < Duration::from_millis(500));
        h.join().unwrap();

        let drained: Vec<safer_ffi::String> = tc_cli_push_drain().into();
        let texts: Vec<String> = drained.iter().map(|s| s.to_string()).collect();
        assert!(texts.iter().any(|s| s == "hello"));
    }

    #[test]
    fn wait_times_out_on_empty() {
        let _guard = TEST_LOCK.lock().unwrap();
        let _ = tc_cli_push_drain();
        let started = std::time::Instant::now();
        assert_eq!(tc_cli_push_wait(30), 0);
        assert!(started.elapsed() >= Duration::from_millis(20));
    }
}
