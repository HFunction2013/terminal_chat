// unignore.rs
// Unignore comments of certain user
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct UnignoreCommand;

impl CommandExecutor for UnignoreCommand {
    fn name(&self) -> &'static str {
        "unignore"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Unignore comments of certain user
        println!("Command `unignore` is not yet implemented.");
        Ok(())
    }
}
