// unsetg.rs
// Unset global variable.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
