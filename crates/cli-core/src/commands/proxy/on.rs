// on.rs
// Turn on system proxy.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct OnCommand;

impl OnCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Turn on system proxy.
		println!("Command `on` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for OnCommand {
	fn name(&self) -> &'static str {
		"on"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
