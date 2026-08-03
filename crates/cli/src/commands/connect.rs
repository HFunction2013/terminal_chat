// connect.rs
// Connect the server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct ConnectCommand;

impl ConnectCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Connect the server
		println!("Command `connect` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for ConnectCommand {
	fn name(&self) -> &'static str {
		"connect"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
