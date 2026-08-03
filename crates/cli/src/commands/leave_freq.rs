// leave_freq.rs
// Leave current frequency.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct LeaveFreqCommand;

impl LeaveFreqCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Leave current frequency.
		println!("Command `leave_freq` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for LeaveFreqCommand {
	fn name(&self) -> &'static str {
		"leave_freq"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
