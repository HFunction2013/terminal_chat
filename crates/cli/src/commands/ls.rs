// ls.rs
// List files in current directory.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct LsCommand;

impl LsCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: List files in current directory.
		println!("Command `ls` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for LsCommand {
	fn name(&self) -> &'static str {
		"ls"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
