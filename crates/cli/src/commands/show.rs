// show.rs
// Show runtime information
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ShowCommand;

impl CommandExecutor for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show runtime information
        println!("Command `show` is not yet implemented.");
        Ok(())
    }
}
