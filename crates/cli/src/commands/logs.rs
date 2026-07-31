#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
#[cfg(debug_assertions)]
// logs.rs
// open/close logging
use clap::ArgMatches;

pub struct LogsCommand;

impl CommandExecutor for LogsCommand {
    fn name(&self) -> &'static str {
        "logs"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: open/close logging
        println!("Command `logs` is not yet implemented.");
        Ok(())
    }
}
