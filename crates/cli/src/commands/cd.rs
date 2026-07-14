// cd.rs
// Change the current remote directory to DIR.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct CdCommand;

impl CommandExecutor for CdCommand {
    fn name(&self) -> &'static str {
        "cd"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Change the current remote directory to DIR.
        println!("Command `cd` is not yet implemented.");
        Ok(())
    }
}
