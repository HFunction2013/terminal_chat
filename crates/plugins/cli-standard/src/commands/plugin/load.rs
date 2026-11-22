// load.rs
// Load a plugin
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LoadCommand;

impl LoadCommand {
    /// `plugin` - Plugin to load, required, value_name: PLUGIN
    #[allow(unused_variables)]
    pub fn execute(&self, plugin: String) -> Result<()> {
        crate::commands::load::plugin::PluginCommand.execute(plugin)
    }
}

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin"))
            })?
            .clone();
        self.execute(plugin)
    }
}
