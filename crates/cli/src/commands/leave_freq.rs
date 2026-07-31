// leave_freq.rs
// Leave current frequency.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct LeaveFreqCommand;

impl CommandExecutor for LeaveFreqCommand {
    fn name(&self) -> &'static str {
        "leave_freq"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Leave current frequency.
        println!("Command `leave_freq` is not yet implemented.");
        Ok(())
    }
}
