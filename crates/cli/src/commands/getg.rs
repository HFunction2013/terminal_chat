// getg.rs
// Get global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct GetgCommand;

impl GetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    #[allow(unused_variables)]
	fn execute(&self, key: String) -> Result<()> {
		// TODO: Get global options
		println!("Command `getg` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for GetgCommand {
	fn name(&self) -> &'static str {
		"getg"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        self.execute(key)
	}
}
