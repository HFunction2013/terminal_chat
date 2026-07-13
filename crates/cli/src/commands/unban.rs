// unban.rs
// unban banned users
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct UnbanCommand;

impl CommandExecutor for UnbanCommand {
    fn name(&self) -> &'static str {
        "unban"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: unban banned users
        println!("Command `unban` is not yet implemented.");
        Ok(())
    }
}
