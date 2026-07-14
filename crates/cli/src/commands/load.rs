// load.rs
// Load Plugin
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct LoadCommand;

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Load Plugin
        println!("Command `load` is not yet implemented.");
        Ok(())
    }
}
