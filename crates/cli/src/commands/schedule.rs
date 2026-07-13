// schedule.rs
// schedule a task
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
