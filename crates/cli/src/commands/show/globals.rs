// globals.rs
// Show value of all global variables
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
