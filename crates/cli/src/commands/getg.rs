// getg.rs
// Get global options
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
