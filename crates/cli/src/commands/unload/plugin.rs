// plugin.rs
// unload plugin
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
