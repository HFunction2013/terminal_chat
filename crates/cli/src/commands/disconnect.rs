// disconnect.rs
// Disconnect from server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
