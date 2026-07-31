// search.rs
// Search plugin in registry
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
