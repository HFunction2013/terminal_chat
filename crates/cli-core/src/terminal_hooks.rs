use ::safer_ffi::prelude::*;
use cli_core_types::{Result, TerminalHookFn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 已注册的 `(suspend, resume)` 钩子地址；`0` 表示该侧未注册。
///
/// 地址由宿主在启动时注册，指向一个稳定的 `extern "C"` 函数；插件经
/// `tc_cli_terminal_suspend` / `tc_cli_terminal_resume` 在外部 TUI 子进程
/// 前后触发，宿主据此离开 / 重进 alternate screen。
static HOOKS: LazyLock<Mutex<(usize, usize)>> = LazyLock::new(|| Mutex::new((0, 0)));

/// 外部子进程是否正在占用终端。置位期间宿主界面应停止读取按键与渲染输出，
/// 把终端完全交给子进程；复位后下一帧整屏重绘。
static BUSY: AtomicBool = AtomicBool::new(false);

fn call_hook(addr: usize) {
    if addr == 0 {
        return;
    }
    // 地址来自宿主注册的稳定 extern "C" 函数；跨线程仅调用、不持有可变状态。
    let hook: TerminalHookFn = unsafe { std::mem::transmute(addr) };
    let _ = hook();
}

/// 注册终端挂起 / 恢复钩子；传 `0` 地址表示清空对应侧。
#[ffi_export]
pub fn tc_cli_set_terminal_hooks(suspend: i64, resume: i64) {
    *HOOKS.lock().unwrap() = (suspend as usize, resume as usize);
}

/// 拉起外部 TUI 子进程前调用：离开 alternate screen、退回原终端态。
#[ffi_export]
pub fn tc_cli_terminal_suspend() -> Result {
    BUSY.store(true, Ordering::SeqCst);
    let (suspend, _) = *HOOKS.lock().unwrap();
    call_hook(suspend);
    Result::success()
}

/// 外部 TUI 子进程退出后调用：重进 alternate screen、恢复宿主界面。
#[ffi_export]
pub fn tc_cli_terminal_resume() -> Result {
    let (_, resume) = *HOOKS.lock().unwrap();
    call_hook(resume);
    BUSY.store(false, Ordering::SeqCst);
    Result::success()
}

/// 终端是否正被外部子进程占用（1 占用中 / 0 空闲）。
#[ffi_export]
pub fn tc_cli_terminal_is_busy() -> i32 {
    BUSY.load(Ordering::SeqCst) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SUSPENDED: AtomicUsize = AtomicUsize::new(0);
    static RESUMED: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn stub_suspend() -> Result {
        SUSPENDED.fetch_add(1, Ordering::SeqCst);
        Result::success()
    }

    extern "C" fn stub_resume() -> Result {
        RESUMED.fetch_add(1, Ordering::SeqCst);
        Result::success()
    }

    #[test]
    fn hooks_run_when_registered_and_are_noop_when_cleared() {
        tc_cli_set_terminal_hooks(
            stub_suspend as *const () as i64,
            stub_resume as *const () as i64,
        );
        let _ = tc_cli_terminal_suspend();
        assert_eq!(tc_cli_terminal_is_busy(), 1);
        let _ = tc_cli_terminal_resume();
        assert_eq!(tc_cli_terminal_is_busy(), 0);
        assert_eq!(SUSPENDED.load(Ordering::SeqCst), 1);
        assert_eq!(RESUMED.load(Ordering::SeqCst), 1);

        // 清空钩子后再次触发：不应解引用空地址，计数保持不变。
        tc_cli_set_terminal_hooks(0, 0);
        let _ = tc_cli_terminal_suspend();
        assert_eq!(tc_cli_terminal_is_busy(), 1);
        let _ = tc_cli_terminal_resume();
        assert_eq!(tc_cli_terminal_is_busy(), 0);
        assert_eq!(SUSPENDED.load(Ordering::SeqCst), 1);
        assert_eq!(RESUMED.load(Ordering::SeqCst), 1);
    }
}
