// plugin.rs
// unload plugin
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct PluginCommand;

impl PluginCommand {
    /// `plugin_name` - Plugin to unload, required, value_name: PLUGIN_NAME
    #[allow(unused_variables)]
	fn execute(&self, plugin_name: String) -> Result<()> {
		// TODO: unload plugin
		println!("Command `plugin` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for PluginCommand {
	fn name(&self) -> &'static str {
		"plugin"
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
