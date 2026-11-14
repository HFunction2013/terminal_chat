//! 外部 TUI 子进程前后的终端全屏切换辅助。
//!
//! 经 cli-core 可选符号通知宿主：拉起抢占终端的外部程序前离开 alternate screen，
//! 退出后重进；符号缺失（旧核心）时静默回退，保持原有行为。

use crate::LIB;

type HookFn = extern "C" fn();

/// 取 cli-core 注册的终端钩子符号；缺失时返回 `None`。
fn load_hook(symbol: &[u8]) -> Option<HookFn> {
    let lib = LIB.get()?;
    unsafe { lib.get::<HookFn>(symbol).ok() }.map(|sym| *sym)
}

/// 拉起外部 TUI 子进程前调用：宿主离开全屏、退回原终端态。
pub fn suspend() {
    if let Some(hook) = load_hook(b"tc_cli_terminal_suspend") {
        hook();
    }
}

/// 外部 TUI 子进程退出后调用：宿主重进全屏并恢复界面。
pub fn resume() {
    if let Some(hook) = load_hook(b"tc_cli_terminal_resume") {
        hook();
    }
}
