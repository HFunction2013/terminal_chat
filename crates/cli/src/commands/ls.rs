// ls.rs
// List files in current directory.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
