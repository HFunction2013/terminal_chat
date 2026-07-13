// unset.rs
// Unset variable.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct UnsetCommand;

impl CommandExecutor for UnsetCommand {
    fn name(&self) -> &'static str {
        "unset"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Unset variable.
        println!("Command `unset` is not yet implemented.");
        Ok(())
    }
}
