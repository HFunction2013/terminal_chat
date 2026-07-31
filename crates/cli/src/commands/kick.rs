// kick.rs
// kick user from freq (op)
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
