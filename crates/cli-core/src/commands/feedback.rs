// feedback.rs
// Send feedback.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct FeedbackCommand;

impl FeedbackCommand {
    #[allow(unused_variables)]
	fn execute(&self) -> Result<()> {
		// TODO: Send feedback.
		println!("Command `feedback` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for FeedbackCommand {
	fn name(&self) -> &'static str {
		"feedback"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
	}
}
