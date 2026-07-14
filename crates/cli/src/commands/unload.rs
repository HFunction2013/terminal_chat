// unload.rs
// Unload Plugin
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UnloadCommand;

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Unload Plugin
        println!("Command `unload` is not yet implemented.");
        Ok(())
    }
}
