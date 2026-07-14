// reconnect.rs
// Reconnect the server
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ReconnectCommand;

impl CommandExecutor for ReconnectCommand {
    fn name(&self) -> &'static str {
        "reconnect"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Reconnect the server
        println!("Command `reconnect` is not yet implemented.");
        Ok(())
    }
}
