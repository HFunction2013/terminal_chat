// disband.rs
// disband current frequency.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
