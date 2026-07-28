// active_freqs.rs
// Show all active frequencies
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ActiveFreqsCommand;

impl CommandExecutor for ActiveFreqsCommand {
    fn name(&self) -> &'static str {
        "active_freqs"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show all active frequencies
        println!("Command `active_freqs` is not yet implemented.");
        Ok(())
    }
}
