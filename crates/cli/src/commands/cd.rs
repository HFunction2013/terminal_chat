// cd.rs
// Change the current remote directory to DIR.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
