// cancel.rs
// cancel a task
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct CancelCommand;

impl CancelCommand {
    /// `task_id` - The task id, required, value_name: TASK_ID
    #[allow(unused_variables)]
	fn execute(&self, task_id: String) -> Result<()> {
		// TODO: cancel a task
		println!("Command `cancel` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for CancelCommand {
	fn name(&self) -> &'static str {
		"cancel"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let task_id = matches
            .get_one::<String>("task_id")
            .ok_or_else(|| anyhow!("Missing required argument: task_id"))?
            .clone();
        self.execute(task_id)
	}
}
