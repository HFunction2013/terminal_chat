// route.rs
// Trace route to server
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct RouteCommand;

impl CommandExecutor for RouteCommand {
    fn name(&self) -> &'static str {
        "route"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Trace route to server
        println!("Command `route` is not yet implemented.");
        Ok(())
    }
}
