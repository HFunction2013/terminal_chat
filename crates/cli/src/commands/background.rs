// background.rs
// Background current session and return to main console
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct BackgroundCommand;

impl CommandExecutor for BackgroundCommand {
    fn name(&self) -> &'static str {
        "background"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Background current session and return to main console
        println!("Command `background` is not yet implemented.");
        Ok(())
    }
}
