// plugin.rs
// load plugin
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct PluginCommand;

impl PluginCommand {
    /// `load_namespace` - load plugin's namespace while loading the plugin
    /// `plugin` - Plugin to load, required, value_name: PLUGIN
    #[allow(unused_variables)]
	fn execute(&self, load_namespace: bool, plugin: String) -> Result<()> {
		// TODO: load plugin
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
        let load_namespace = matches.get_flag("load_namespace");
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| anyhow!("Missing required argument: plugin"))?
            .clone();
        self.execute(load_namespace, plugin)
	}
}
