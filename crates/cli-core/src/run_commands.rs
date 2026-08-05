use crate::IN_CMD;
use crate::commands;
use anyhow::Result;
use clap::Command;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
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
fn internal_run_command(args: &[String]) -> Result<()> {
    let full_args: Vec<&str> =
        std::iter::once("tc-cli").chain(args.iter().map(String::as_str)).collect();

    match (*CLI).clone().try_get_matches_from(&full_args) {
        Ok(matches) => {
            IN_CMD.store(true, Ordering::SeqCst);
            let _guard = CommandGuard;
            let result = commands::dispatch(&matches);
            if let Err(ref e) = result {
                eprintln!("Error: {e}");
            }
            result
        }
        Err(err) => Err(err.into()),
    }
}
crate::define_hook_system!(
    internal_run_command,
    "run_command",
    M,
    R,
    R,
    R,
    &mut Vec<String>,
    &Vec<String>,
    &Vec<String>,
    Vec<String>,
    Result<()>
);
