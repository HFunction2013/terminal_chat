// schedule.rs
// schedule a task
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct ScheduleCommand;

impl ScheduleCommand {
    /// `session` - The certain session, 0 for main console., required, value_name: SESSION
    /// `time` - send time, required, value_name: TIME
    /// `cmd` - Dest CMD, #EDITOR# to open editor, required, value_name: CMD
    #[allow(unused_variables)]
	fn execute(&self, session: String, time: String, cmd: String) -> Result<()> {
		// TODO: schedule a task
		println!("Command `schedule` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for ScheduleCommand {
	fn name(&self) -> &'static str {
		"schedule"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let session = matches
            .get_one::<String>("session")
            .ok_or_else(|| anyhow!("Missing required argument: session"))?
            .clone();
        let time = matches
            .get_one::<String>("time")
            .ok_or_else(|| anyhow!("Missing required argument: time"))?
            .clone();
        let cmd = matches
            .get_one::<String>("cmd")
            .ok_or_else(|| anyhow!("Missing required argument: cmd"))?
            .clone();
        self.execute(session, time, cmd)
	}
}
