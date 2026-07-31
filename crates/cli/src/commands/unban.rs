// unban.rs
// unban banned users
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
