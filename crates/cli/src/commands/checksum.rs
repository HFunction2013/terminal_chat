// checksum.rs
// Get checksum of remote file.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct ChecksumCommand;

impl CommandExecutor for ChecksumCommand {
    fn name(&self) -> &'static str {
        "checksum"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get checksum of remote file.
        println!("Command `checksum` is not yet implemented.");
        Ok(())
    }
}
