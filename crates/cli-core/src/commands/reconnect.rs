// reconnect.rs
// Reconnect the server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct ReconnectCommand;

impl ReconnectCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Reconnect the server
		println!("Command `reconnect` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for ReconnectCommand {
	fn name(&self) -> &'static str {
		"reconnect"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
