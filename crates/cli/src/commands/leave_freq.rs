// leave_freq.rs
// Leave current frequency.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
