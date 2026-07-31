// options.rs
// Show available set/setg options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
