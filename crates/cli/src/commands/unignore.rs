// unignore.rs
// Unignore comments of certain user
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
