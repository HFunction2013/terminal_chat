// mkdir.rs
// Make directory in remote server.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
