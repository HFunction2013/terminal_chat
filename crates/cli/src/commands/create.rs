// create.rs
// Create alias or marco
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct CreateCommand;

impl CommandExecutor for CreateCommand {
    fn name(&self) -> &'static str {
        "create"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Create alias or marco
        println!("Command `create` is not yet implemented.");
        Ok(())
    }
}
