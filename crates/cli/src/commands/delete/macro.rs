// macro.rs
// Delete macro
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
        // TODO: Delete macro
        println!("Command `macro` is not yet implemented.");
        Ok(())
    }
}
