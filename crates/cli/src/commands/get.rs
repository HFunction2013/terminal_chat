// get.rs
// Get session options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
