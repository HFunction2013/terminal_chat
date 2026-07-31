// ignore.rs
// Ignore comments of certain user
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
