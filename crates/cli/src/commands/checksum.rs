// checksum.rs
// Get checksum of remote file (SHA256).
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ChecksumCommand;

impl CommandExecutor for ChecksumCommand {
    fn name(&self) -> &'static str {
        "checksum"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get checksum of remote file (SHA256).
        println!("Command `checksum` is not yet implemented.");
        Ok(())
    }
}
