// updates.rs
// Show all avaliable updates.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct UpdatesCommand;

impl CommandExecutor for UpdatesCommand {
    fn name(&self) -> &'static str {
        "updates"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show all avaliable updates.
        println!("Command `updates` is not yet implemented.");
        Ok(())
    }
}
