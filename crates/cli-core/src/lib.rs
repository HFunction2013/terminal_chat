pub mod _void;
pub mod global_settings;
mod hook;
pub mod print_content;
pub use _void::*;
#[macro_use]
mod hook_macros;

use std::sync::atomic::AtomicBool;

pub mod cli_command;
pub mod commands;
pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static IN_CMD: AtomicBool = AtomicBool::new(false);
