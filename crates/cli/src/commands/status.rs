// status.rs
// Test network
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct StatusCommand;

impl StatusCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Test network
		println!("Command `status` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for StatusCommand {
	fn name(&self) -> &'static str {
		"status"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
