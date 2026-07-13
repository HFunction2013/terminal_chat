// schedule_msg.rs
// Schedules message.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct ScheduleMsgCommand;

impl CommandExecutor for ScheduleMsgCommand {
    fn name(&self) -> &'static str {
        "schedule_msg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Schedules message.
        println!("Command `schedule_msg` is not yet implemented.");
        Ok(())
    }
}
