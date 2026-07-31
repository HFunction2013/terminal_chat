// macro.rs
// Create macro
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct MacroCommand;

impl CommandExecutor for MacroCommand {
    fn name(&self) -> &'static str {
        "macro"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Create macro
        println!("Command `macro` is not yet implemented.");
        Ok(())
    }
}
