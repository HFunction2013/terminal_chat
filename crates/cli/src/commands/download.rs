// download.rs
// Download file or folder to server.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct DownloadCommand;

impl CommandExecutor for DownloadCommand {
    fn name(&self) -> &'static str {
        "download"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Download file or folder to server.
        println!("Command `download` is not yet implemented.");
        Ok(())
    }
}
