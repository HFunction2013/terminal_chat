// show.rs
// Show plugin information from registry(default)
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct ShowCommand;

impl CommandExecutor for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show plugin information from registry(default)
        println!("Command `show` is not yet implemented.");
        Ok(())
    }
}
