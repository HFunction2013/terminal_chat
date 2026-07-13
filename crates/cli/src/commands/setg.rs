// setg.rs
// Set global options
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct SetgCommand;

impl CommandExecutor for SetgCommand {
    fn name(&self) -> &'static str {
        "setg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Set global options
        println!("Command `setg` is not yet implemented.");
        Ok(())
    }
}
