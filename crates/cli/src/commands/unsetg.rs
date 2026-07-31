// unsetg.rs
// Unset global variable.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct UnsetgCommand;

impl CommandExecutor for UnsetgCommand {
    fn name(&self) -> &'static str {
        "unsetg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Unset global variable.
        println!("Command `unsetg` is not yet implemented.");
        Ok(())
    }
}
