// trop.rs
// transfer creator permission.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct TropCommand;

impl CommandExecutor for TropCommand {
    fn name(&self) -> &'static str {
        "trop"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: transfer creator permission.
        println!("Command `trop` is not yet implemented.");
        Ok(())
    }
}
