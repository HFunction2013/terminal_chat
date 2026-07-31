// loaded.rs
// show all loaded plugins.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
