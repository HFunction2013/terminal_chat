// loaded.rs
// show all loaded plugins.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct LoadedCommand;

impl CommandExecutor for LoadedCommand {
    fn name(&self) -> &'static str {
        "loaded"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: show all loaded plugins.
        println!("Command `loaded` is not yet implemented.");
        Ok(())
    }
}
