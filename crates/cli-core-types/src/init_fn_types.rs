use crate::Result;

pub type IsInterruptedFn = extern "C" fn() -> Result;
pub type IsInCmdFn = extern "C" fn() -> Result;
pub type SetInterruptedFn = extern "C" fn(bool) -> Result;
pub type SetInCmdFn = extern "C" fn(bool) -> Result;
