// search.rs
// Search plugin in registry
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct SearchCommand;

impl SearchCommand {
    /// `plugin_name` - plugin name to search, required, value_name: PLUGIN_NAME
    #[allow(unused_variables)]
	fn execute(&self, plugin_name: String) -> Result<()> {
		// TODO: Search plugin in registry
		println!("Command `search` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for SearchCommand {
	fn name(&self) -> &'static str {
		"search"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin_name = matches
            .get_one::<String>("plugin_name")
            .ok_or_else(|| anyhow!("Missing required argument: plugin_name"))?
            .clone();
        self.execute(plugin_name)
	}
}
