// create_freq.rs
// Create a new frequency channel
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct CreateFreqCommand;

impl CommandExecutor for CreateFreqCommand {
    fn name(&self) -> &'static str {
        "create_freq"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Create a new frequency channel
        println!("Command `create_freq` is not yet implemented.");
        Ok(())
    }
}
