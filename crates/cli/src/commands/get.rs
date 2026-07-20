// get.rs
// Get session options
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct GetCommand;

impl CommandExecutor for GetCommand {
    fn name(&self) -> &'static str {
        "get"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get session options
        println!("Command `get` is not yet implemented.");
        Ok(())
    }
}
