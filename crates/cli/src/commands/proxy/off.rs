// off.rs
// Turn off system proxy.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct OffCommand;

impl CommandExecutor for OffCommand {
    fn name(&self) -> &'static str {
        "off"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Turn off system proxy.
        println!("Command `off` is not yet implemented.");
        Ok(())
    }
}
