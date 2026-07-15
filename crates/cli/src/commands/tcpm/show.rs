// show.rs
// Show plugin information from registry(default)
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
        // TODO: Show plugin information from registry(default)
        println!("Command `show` is not yet implemented.");
        Ok(())
    }
}
