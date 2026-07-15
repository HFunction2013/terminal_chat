// updates.rs
// Show all avaliable updates.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
