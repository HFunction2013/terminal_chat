// setg.rs
// Set global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct SetgCommand;

impl SetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    /// `value` - Target config value, value_name: VALUE
    /// `password` - use rpassword to read the value, conflicts with: value
    #[allow(unused_variables)]
	fn execute(&self, key: String, value: Option<String>, password: bool) -> Result<()> {
		// TODO: Set global options
		println!("Command `setg` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for SetgCommand {
	fn name(&self) -> &'static str {
		"setg"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        let value = matches
            .get_one::<String>("value")
            .cloned();
        let password = matches.get_flag("password");
        self.execute(key, value, password)
	}
}
