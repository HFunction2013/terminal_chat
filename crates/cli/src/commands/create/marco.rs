// marco.rs
// Create marco
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct MarcoCommand;

impl CommandExecutor for MarcoCommand {
    fn name(&self) -> &'static str {
        "marco"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Create marco
        println!("Command `marco` is not yet implemented.");
        Ok(())
    }
}
