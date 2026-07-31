// background.rs
// Background current session and return to main console
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
