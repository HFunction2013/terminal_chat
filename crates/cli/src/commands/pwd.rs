// pwd.rs
// Get current remote directory.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct PwdCommand;

impl CommandExecutor for PwdCommand {
    fn name(&self) -> &'static str {
        "pwd"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get current remote directory.
        println!("Command `pwd` is not yet implemented.");
        Ok(())
    }
}
