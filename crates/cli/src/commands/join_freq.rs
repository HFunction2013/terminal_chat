// join_freq.rs
// Join a target frequency channel
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
