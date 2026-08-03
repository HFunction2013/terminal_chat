// sleep.rs
// Are you sleepy? Why not sleep for a while?
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct SleepCommand;

impl SleepCommand {
    /// `milliseconds` - Aha... sleepy!, required, value_name: TIME, default: 1000
    #[allow(unused_variables)]
	fn execute(&self, milliseconds: humantime::Duration) -> Result<()> {
		// TODO: Are you sleepy? Why not sleep for a while?
		println!("Command `sleep` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for SleepCommand {
	fn name(&self) -> &'static str {
		"sleep"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let milliseconds = matches
            .get_one::<humantime::Duration>("milliseconds")
            .ok_or_else(|| anyhow!("Missing required argument: milliseconds"))?
            .clone();
        self.execute(milliseconds)
	}
}
