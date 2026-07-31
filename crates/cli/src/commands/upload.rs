// upload.rs
// Upload file or folder to server.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
