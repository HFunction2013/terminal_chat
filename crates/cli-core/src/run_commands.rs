use crate::IN_CMD;
use crate::commands;
use crate::result::Result;
use ::safer_ffi::prelude::*;
use clap::Command;
use hook_macro::register_hook;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
mod command {
    include!(concat!(env!("OUT_DIR"), "/command.rs"));
}

struct CommandGuard;
impl Drop for CommandGuard {
    fn drop(&mut self) {
        IN_CMD.store(false, Ordering::SeqCst);
    }
}

pub fn build_cli() -> Command {
    command::add_commands(Command::new("tc-cli").version(env!("CARGO_PKG_VERSION")))
}

pub static CLI: LazyLock<Command> = LazyLock::new(build_cli);

#[register_hook]
fn run_command_impl(args: &repr_c::Vec<repr_c::String>) -> Result {
    let full_args: Vec<String> =
        std::iter::once("tc-cli".to_string()).chain(args.iter().map(|s| s.to_string())).collect();

    let full_args_refs: Vec<&str> = full_args.iter().map(String::as_str).collect();

    match (*CLI).clone().try_get_matches_from(&full_args_refs) {
        Ok(matches) => {
            IN_CMD.store(true, Ordering::SeqCst);
            let _guard = CommandGuard;
            let result = commands::dispatch(&matches);
            match result {
                Ok(()) => Result::success(),
                Err(e) => Result::error(&e.to_string()),
            }
        }
        Err(err) => Result::error(&err.to_string()),
    }
}
