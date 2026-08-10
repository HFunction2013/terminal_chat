use std::sync::atomic::AtomicBool;
pub mod global_settings;
pub mod print_content;
pub mod commands;
pub mod macros;
pub mod run_commands;
pub mod result;

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static IN_CMD: AtomicBool = AtomicBool::new(false);
