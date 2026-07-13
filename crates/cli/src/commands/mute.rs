// mute.rs
// mute a certain user, use `set QUIET true` to mute everyboy except mods.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
