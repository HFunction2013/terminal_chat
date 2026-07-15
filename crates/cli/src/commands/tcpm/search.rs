// search.rs
// Search plugin in registry
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct SearchCommand;

impl CommandExecutor for SearchCommand {
    fn name(&self) -> &'static str {
        "search"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Search plugin in registry
        println!("Command `search` is not yet implemented.");
        Ok(())
    }
}
