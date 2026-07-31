// freq_info.rs
// Get frequency information
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
