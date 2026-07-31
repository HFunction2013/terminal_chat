// getg.rs
// Get global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct GetgCommand;

impl CommandExecutor for GetgCommand {
    fn name(&self) -> &'static str {
        "getg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get global options
        println!("Command `getg` is not yet implemented.");
        Ok(())
    }
}
