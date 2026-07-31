// download.rs
// Download plugin
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
        // TODO: Download plugin
        println!("Command `download` is not yet implemented.");
        Ok(())
    }
}
