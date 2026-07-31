// unmute.rs
// mute reversed.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
