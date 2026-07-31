// clear.rs
// Clear the Screen
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
