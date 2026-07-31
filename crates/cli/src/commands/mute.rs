// mute.rs
// mute a certain user, use `set QUIET true` to mute everyboy except mods.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct MuteCommand;

impl CommandExecutor for MuteCommand {
    fn name(&self) -> &'static str {
        "mute"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: mute a certain user, use `set QUIET true` to mute everyboy except mods.
        println!("Command `mute` is not yet implemented.");
        Ok(())
    }
}
