// cancel.rs
// cancel a task
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
