// tasks.rs
// Get scheduled tasks.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct TasksCommand;

impl CommandExecutor for TasksCommand {
    fn name(&self) -> &'static str {
        "tasks"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get scheduled tasks.
        println!("Command `tasks` is not yet implemented.");
        Ok(())
    }
}
