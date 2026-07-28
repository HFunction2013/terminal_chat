// moderators.rs
// Show moderators in current frequency
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ModeratorsCommand;

impl CommandExecutor for ModeratorsCommand {
    fn name(&self) -> &'static str {
        "moderators"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show moderators in current frequency
        println!("Command `moderators` is not yet implemented.");
        Ok(())
    }
}
