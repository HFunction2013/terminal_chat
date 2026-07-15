// ausers.rs
// Show active users in current frequency
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct AusersCommand;

impl CommandExecutor for AusersCommand {
    fn name(&self) -> &'static str {
        "ausers"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show active users in current frequency
        println!("Command `ausers` is not yet implemented.");
        Ok(())
    }
}
