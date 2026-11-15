//! 外部 TUI 子进程前后的终端全屏切换辅助。
//!
//! 经 cli-core 可选符号通知宿主：拉起抢占终端的外部程序前离开 alternate screen，
//! 退出后重进；符号缺失（旧核心）时静默回退，保持原有行为。

use crate::LIB;
use cli_core_types::TerminalHookFn;

/// 取 cli-core 注册的终端钩子符号；缺失时返回 `None`。
///
/// 签名必须与 cli-core 导出的 `tc_cli_terminal_suspend/resume` 一致：二者都返回
/// `Result`（含堆字符串的结构体，Windows x64 经隐藏返回指针 sret 传递）。若在此
/// 误声明为无返回值 `fn()`，调用方不会传入 sret 指针，被调方写返回值时解引用野址，
/// 在 Windows 上表现为 `0xc0000005` 访问违规（崩在 cli_core.dll 钩子函数体内）。
fn load_hook(symbol: &[u8]) -> Option<TerminalHookFn> {
    let lib = LIB.get()?;
    unsafe { lib.get::<TerminalHookFn>(symbol).ok() }.map(|sym| *sym)
}

/// 拉起外部 TUI 子进程前调用：宿主离开全屏、退回原终端态。
pub fn suspend() {
    if let Some(hook) = load_hook(b"tc_cli_terminal_suspend") {
        let _ = hook();
    }
}

/// 外部 TUI 子进程退出后调用：宿主重进全屏并恢复界面。
pub fn resume() {
    if let Some(hook) = load_hook(b"tc_cli_terminal_resume") {
        let _ = hook();
    }
}
