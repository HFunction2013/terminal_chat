// disconnect.rs
// Disconnect from server
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct DisconnectCommand;

impl CommandExecutor for DisconnectCommand {
    fn name(&self) -> &'static str {
        "disconnect"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Disconnect from server
        println!("Command `disconnect` is not yet implemented.");
        Ok(())
    }
}
