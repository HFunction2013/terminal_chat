// on.rs
// Turn on system proxy.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct OnCommand;

impl CommandExecutor for OnCommand {
    fn name(&self) -> &'static str {
        "on"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Turn on system proxy.
        println!("Command `on` is not yet implemented.");
        Ok(())
    }
}
