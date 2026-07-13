// ignore.rs
// Ignore comments of certain user
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct IgnoreCommand;

impl CommandExecutor for IgnoreCommand {
    fn name(&self) -> &'static str {
        "ignore"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Ignore comments of certain user
        println!("Command `ignore` is not yet implemented.");
        Ok(())
    }
}
