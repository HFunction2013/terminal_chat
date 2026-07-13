// ban.rs
// ban user from freq
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct BanCommand;

impl CommandExecutor for BanCommand {
    fn name(&self) -> &'static str {
        "ban"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: ban user from freq
        println!("Command `ban` is not yet implemented.");
        Ok(())
    }
}
