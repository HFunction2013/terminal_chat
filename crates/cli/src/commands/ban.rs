// ban.rs
// ban user from freq
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
