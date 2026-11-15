// unload.rs
// Unload a plugin
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UnloadCommand;

impl UnloadCommand {
    /// `plugin_name` - Plugin to unload, required, value_name: PLUGIN_NAME
    #[allow(unused_variables)]
    pub fn execute(&self, plugin_name: String) -> Result<()> {
        crate::commands::unload::plugin::PluginCommand.execute(plugin_name)
    }
}

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
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
