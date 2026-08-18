pub type IsInterruptedFn = extern "C" fn() -> bool;
pub type IsInCmdFn = extern "C" fn() -> bool;
pub type SetInterruptedFn = extern "C" fn(bool) -> bool;
pub type SetInCmdFn = extern "C" fn(bool) -> bool;
