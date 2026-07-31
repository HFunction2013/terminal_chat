// join_freq.rs
// Join a target frequency channel
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct JoinFreqCommand;

impl CommandExecutor for JoinFreqCommand {
    fn name(&self) -> &'static str {
        "join_freq"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Join a target frequency channel
        println!("Command `join_freq` is not yet implemented.");
        Ok(())
    }
}
