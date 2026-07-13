// status.rs
// Test network
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct StatusCommand;

impl CommandExecutor for StatusCommand {
    fn name(&self) -> &'static str {
        "status"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Test network
        println!("Command `status` is not yet implemented.");
        Ok(())
    }
}
