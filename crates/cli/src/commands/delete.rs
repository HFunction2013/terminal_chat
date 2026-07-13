// delete.rs
// delete a alias or marco
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct DeleteCommand;

impl CommandExecutor for DeleteCommand {
    fn name(&self) -> &'static str {
        "delete"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: delete a alias or marco
        println!("Command `delete` is not yet implemented.");
        Ok(())
    }
}
