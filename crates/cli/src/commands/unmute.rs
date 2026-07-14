// unmute.rs
// mute reversed.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UnmuteCommand;

impl CommandExecutor for UnmuteCommand {
    fn name(&self) -> &'static str {
        "unmute"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: mute reversed.
        println!("Command `unmute` is not yet implemented.");
        Ok(())
    }
}
