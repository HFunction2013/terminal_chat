// globals.rs
// Show value of all global variables
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct GlobalsCommand;

impl CommandExecutor for GlobalsCommand {
    fn name(&self) -> &'static str {
        "globals"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show value of all global variables
        println!("Command `globals` is not yet implemented.");
        Ok(())
    }
}
