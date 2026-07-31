// connect.rs
// Connect the server
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct ConnectCommand;

impl CommandExecutor for ConnectCommand {
    fn name(&self) -> &'static str {
        "connect"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Connect the server
        println!("Command `connect` is not yet implemented.");
        Ok(())
    }
}
