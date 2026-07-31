// cancel.rs
// cancel a task
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct CancelCommand;

impl CommandExecutor for CancelCommand {
    fn name(&self) -> &'static str {
        "cancel"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: cancel a task
        println!("Command `cancel` is not yet implemented.");
        Ok(())
    }
}
