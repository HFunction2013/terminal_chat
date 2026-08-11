use crate::IN_CMD;
use crate::commands;
use crate::result::Result;
use ::safer_ffi::prelude::*;
use clap::Command;
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

// ===== run_command 钩子系统 =====
type BeforeRunCommandHook = unsafe extern "C" fn(*mut repr_c::Vec<repr_c::String>) -> bool;
type OnRunCommandHook = unsafe extern "C" fn(*const repr_c::Vec<repr_c::String>) -> bool;
type AfterRunCommandHook = unsafe extern "C" fn(*const repr_c::Vec<repr_c::String>) -> bool;

static BEFORE_RUN_COMMAND_HOOKS: LazyLock<Mutex<Vec<BeforeRunCommandHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static ON_RUN_COMMAND_HOOKS: LazyLock<Mutex<Vec<OnRunCommandHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static AFTER_RUN_COMMAND_HOOKS: LazyLock<Mutex<Vec<AfterRunCommandHook>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static IN_RUN_COMMAND: AtomicBool = AtomicBool::new(false);

#[ffi_export]
pub fn register_before_run_command(hook: BeforeRunCommandHook) {
    if let Ok(mut hooks) = BEFORE_RUN_COMMAND_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_on_run_command(hook: OnRunCommandHook) {
    if let Ok(mut hooks) = ON_RUN_COMMAND_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn register_after_run_command(hook: AfterRunCommandHook) {
    if let Ok(mut hooks) = AFTER_RUN_COMMAND_HOOKS.lock() {
        hooks.push(hook);
    }
}

#[ffi_export]
pub fn clear_run_command_hooks() {
    let _ = BEFORE_RUN_COMMAND_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_RUN_COMMAND_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_RUN_COMMAND_HOOKS.lock().map(|mut h| h.clear());
}

unsafe fn run_before_run_command_chain(buf: &mut repr_c::Vec<repr_c::String>) -> bool {
    if let Ok(list) = BEFORE_RUN_COMMAND_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *mut repr_c::Vec<repr_c::String>) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_on_run_command_chain(buf: &repr_c::Vec<repr_c::String>) -> bool {
    if let Ok(list) = ON_RUN_COMMAND_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const repr_c::Vec<repr_c::String>) {
                    return true;
                }
            }
        }
    }
    false
}

unsafe fn run_after_run_command_chain(buf: &repr_c::Vec<repr_c::String>) -> bool {
    if let Ok(list) = AFTER_RUN_COMMAND_HOOKS.lock() {
        for h in list.iter() {
            unsafe {
                if h(buf as *const repr_c::Vec<repr_c::String>) {
                    return true;
                }
            }
        }
    }
    false
}

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

#[ffi_export]
pub unsafe fn run_command(content: repr_c::Vec<repr_c::String>) -> Result {
    let reenter = IN_RUN_COMMAND.load(Ordering::SeqCst);
    if reenter {
        return run_command_impl(&content);
    }
    IN_RUN_COMMAND.store(true, Ordering::SeqCst);

    let mut buf = content.clone();
    let mut interrupted = unsafe { run_before_run_command_chain(&mut buf) };

    if !interrupted {
        interrupted = unsafe { run_on_run_command_chain(&buf) };
    }

    let result = if !interrupted {
        let res = run_command_impl(&buf);
        unsafe { run_after_run_command_chain(&buf) };
        res
    } else {
        Result::error("Command execution was interrupted by hook")
    };

    IN_RUN_COMMAND.store(false, Ordering::SeqCst);
    result
}
