// connect.rs
// Connect the server
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
