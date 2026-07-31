// foreground.rs
// Switch to session if there is only one.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct ForegroundCommand;

impl CommandExecutor for ForegroundCommand {
    fn name(&self) -> &'static str {
        "foreground"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Switch to session if there is only one.
        println!("Command `foreground` is not yet implemented.");
        Ok(())
    }
}
