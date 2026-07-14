// disband.rs
// disband current frequency.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct DisbandCommand;

impl CommandExecutor for DisbandCommand {
    fn name(&self) -> &'static str {
        "disband"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: disband current frequency.
        println!("Command `disband` is not yet implemented.");
        Ok(())
    }
}
