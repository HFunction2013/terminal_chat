// clear.rs
// Clear the Screen
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct ClearCommand;

impl ClearCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Clear the Screen
		println!("Command `clear` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for ClearCommand {
	fn name(&self) -> &'static str {
		"clear"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
