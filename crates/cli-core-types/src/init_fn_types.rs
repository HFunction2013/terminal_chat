use crate::Result;

pub type IsInterruptedFn = extern "C" fn() -> Result;
pub type IsInCmdFn = extern "C" fn() -> Result;
pub type SetInterruptedFn = extern "C" fn(bool) -> Result;
pub type SetInCmdFn = extern "C" fn(bool) -> Result;

/// 终端挂起 / 恢复钩子：宿主在拉起外部 TUI 子进程前后分别调用，
/// 用于离开 / 重进 alternate screen 并切换终端模式。`0` 地址表示未注册。
pub type TerminalHookFn = extern "C" fn() -> Result;
