// clear.rs
// Clear the Screen
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct ClearCommand;

impl CommandExecutor for ClearCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Clear the Screen
        println!("Command `clear` is not yet implemented.");
        Ok(())
    }
}
