// status.rs
// Test network
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
