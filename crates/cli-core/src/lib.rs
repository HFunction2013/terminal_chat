use std::sync::atomic::AtomicBool;
pub mod _void;
pub mod global_settings;
pub mod print_content;
pub use _void::*;
pub mod commands;
pub mod macros;
pub mod run_commands;

mod hook;

#[macro_use]
mod hook_macros;

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static IN_CMD: AtomicBool = AtomicBool::new(false);
