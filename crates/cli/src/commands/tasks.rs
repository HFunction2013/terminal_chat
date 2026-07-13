// tasks.rs
// Get scheduled tasks.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
