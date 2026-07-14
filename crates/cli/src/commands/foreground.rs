// foreground.rs
// Switch to session if there is only one.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
