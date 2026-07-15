// afreqs.rs
// Show all active frequencies
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct AfreqsCommand;

impl CommandExecutor for AfreqsCommand {
    fn name(&self) -> &'static str {
        "afreqs"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show all active frequencies
        println!("Command `afreqs` is not yet implemented.");
        Ok(())
    }
}
