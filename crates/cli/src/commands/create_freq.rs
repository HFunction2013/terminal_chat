// create_freq.rs
// Create a new frequency channel
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
