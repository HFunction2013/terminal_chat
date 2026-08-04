// background.rs
// Background current session and return to main console
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct BackgroundCommand;

impl BackgroundCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Background current session and return to main console
		println!("Command `background` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for BackgroundCommand {
	fn name(&self) -> &'static str {
		"background"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
