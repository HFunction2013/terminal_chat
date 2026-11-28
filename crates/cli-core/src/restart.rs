//! 客户端重启请求。
//!
//! 软重启（`restart`，无参）在进程内重载从磁盘回读的运行态（别名 / 宏 / 配置），
//! 不经过本模块；本模块只承载硬重启信号：
//!
//! - 插件命令 `restart --hard` 调用 [`request_hard_restart`] 置位；
//! - 宿主主线程轮询 [`take_hard_restart_request`]，取出请求后以相同参数自拉起新进程。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use hook_macro::register_hook;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 硬重启待处理标志。
static HARD_RESTART: AtomicBool = AtomicBool::new(false);

/// 请求硬重启：置位标志，等待宿主主线程取走并自拉起进程。
#[register_hook]
fn request_hard_restart() -> Result {
    HARD_RESTART.store(true, Ordering::SeqCst);
    Result::success()
}

/// 宿主轮询：原子取出并清除硬重启请求；返回 `1` 表示有待处理的硬重启。
#[ffi_export]
pub fn take_hard_restart_request() -> i32 {
    HARD_RESTART.swap(false, Ordering::SeqCst) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_sets_flag_then_take_clears_once() {
        // 初始无待处理请求。
        assert_eq!(take_hard_restart_request(), 0);

        unsafe {
            let _ = request_hard_restart();
        }
        // 首次取出命中请求，并在同一原子交换中复位。
        assert_eq!(take_hard_restart_request(), 1);
        // 复位后再次取出不再命中。
        assert_eq!(take_hard_restart_request(), 0);
    }
}
