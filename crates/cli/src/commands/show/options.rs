// options.rs
// Show available set/setg options
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct OptionsCommand;

impl CommandExecutor for OptionsCommand {
    fn name(&self) -> &'static str {
        "options"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show available set/setg options
        println!("Command `options` is not yet implemented.");
        Ok(())
    }
}
