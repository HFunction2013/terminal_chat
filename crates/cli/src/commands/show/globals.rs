// globals.rs
// Show value of all global variables
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct GlobalsCommand;

impl GlobalsCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Show value of all global variables
		println!("Command `globals` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for GlobalsCommand {
	fn name(&self) -> &'static str {
		"globals"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
