// away.rs
// Set your status to be busy or away
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct AwayCommand;

impl CommandExecutor for AwayCommand {
    fn name(&self) -> &'static str {
        "away"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Set your status to be busy or away
        println!("Command `away` is not yet implemented.");
        Ok(())
    }
}
