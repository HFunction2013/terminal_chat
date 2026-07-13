// kick.rs
// kick user from freq (op)
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct KickCommand;

impl CommandExecutor for KickCommand {
    fn name(&self) -> &'static str {
        "kick"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: kick user from freq (op)
        println!("Command `kick` is not yet implemented.");
        Ok(())
    }
}
