// set.rs
// Set session options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct SetCommand;

impl CommandExecutor for SetCommand {
    fn name(&self) -> &'static str {
        "set"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Set session options
        println!("Command `set` is not yet implemented.");
        Ok(())
    }
}
