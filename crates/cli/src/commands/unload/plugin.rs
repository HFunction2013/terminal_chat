// plugin.rs
// unload plugin
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct PluginCommand;

impl CommandExecutor for PluginCommand {
    fn name(&self) -> &'static str {
        "plugin"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: unload plugin
        println!("Command `plugin` is not yet implemented.");
        Ok(())
    }
}
