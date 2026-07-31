// update.rs
// Update plugins.
// 'standard' means main program.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
