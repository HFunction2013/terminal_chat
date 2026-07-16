// macro.rs
// Create macro
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
