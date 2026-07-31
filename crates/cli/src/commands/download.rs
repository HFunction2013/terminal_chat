// download.rs
// Download file or folder to server.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
