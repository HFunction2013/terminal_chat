// route.rs
// Trace route to server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
