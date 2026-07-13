#[cfg(debug_assertions)]
// logs.rs
// open/close logging
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
