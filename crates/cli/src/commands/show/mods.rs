// mods.rs
// Show moderators in current frequency
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ModsCommand;

impl CommandExecutor for ModsCommand {
    fn name(&self) -> &'static str {
        "mods"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show moderators in current frequency
        println!("Command `mods` is not yet implemented.");
        Ok(())
    }
}
