// moderators.rs
// Show moderators in current frequency
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
