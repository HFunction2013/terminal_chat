use std::sync::atomic::AtomicBool;

pub mod cli_command;
pub mod commands;
pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);
pub static IN_CMD: AtomicBool = AtomicBool::new(false);
