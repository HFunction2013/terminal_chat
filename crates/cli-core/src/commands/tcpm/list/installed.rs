// installed.rs
// show all installed plugins.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct InstalledCommand;

impl InstalledCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: show all installed plugins.
		println!("Command `installed` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for InstalledCommand {
	fn name(&self) -> &'static str {
		"installed"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
