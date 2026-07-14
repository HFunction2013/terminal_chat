// upload.rs
// Upload file or folder to server.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UploadCommand;

impl CommandExecutor for UploadCommand {
    fn name(&self) -> &'static str {
        "upload"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Upload file or folder to server.
        println!("Command `upload` is not yet implemented.");
        Ok(())
    }
}
