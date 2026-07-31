// schedule.rs
// schedule a task
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct ScheduleCommand;

impl CommandExecutor for ScheduleCommand {
    fn name(&self) -> &'static str {
        "schedule"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: schedule a task
        println!("Command `schedule` is not yet implemented.");
        Ok(())
    }
}
