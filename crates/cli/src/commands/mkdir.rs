// mkdir.rs
// Make directory in remote server.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct MkdirCommand;

impl CommandExecutor for MkdirCommand {
    fn name(&self) -> &'static str {
        "mkdir"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Make directory in remote server.
        println!("Command `mkdir` is not yet implemented.");
        Ok(())
    }
}
