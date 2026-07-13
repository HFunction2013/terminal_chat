// freq_info.rs
// Get frequency information
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct FreqInfoCommand;

impl CommandExecutor for FreqInfoCommand {
    fn name(&self) -> &'static str {
        "freq_info"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get frequency information
        println!("Command `freq_info` is not yet implemented.");
        Ok(())
    }
}
