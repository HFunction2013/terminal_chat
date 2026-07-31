// unset.rs
// Unset variable.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
