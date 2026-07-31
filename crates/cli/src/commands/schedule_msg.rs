// schedule_msg.rs
// Schedules message.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
