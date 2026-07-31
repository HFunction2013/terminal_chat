// reconnect.rs
// Reconnect the server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
