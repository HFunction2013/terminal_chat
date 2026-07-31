// active_freqs.rs
// Show all active frequencies
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
