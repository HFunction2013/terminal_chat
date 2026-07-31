// ls.rs
// List files in current directory.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct LsCommand;

impl CommandExecutor for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: List files in current directory.
        println!("Command `ls` is not yet implemented.");
        Ok(())
    }
}
