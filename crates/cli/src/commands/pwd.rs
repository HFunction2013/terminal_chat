// pwd.rs
// Get current remote directory.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
