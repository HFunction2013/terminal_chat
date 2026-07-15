// update.rs
// Update plugins.
// 'standard' means main program.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UpdateCommand;

impl CommandExecutor for UpdateCommand {
    fn name(&self) -> &'static str {
        "update"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Update plugins.
        // TODO: 'standard' means main program.
        println!("Command `update` is not yet implemented.");
        Ok(())
    }
}
