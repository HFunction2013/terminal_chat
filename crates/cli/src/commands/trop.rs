// trop.rs
// transfer creator permission.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
