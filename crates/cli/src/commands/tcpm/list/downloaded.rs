// downloaded.rs
// show all downloaded plugins.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct DownloadedCommand;

impl CommandExecutor for DownloadedCommand {
    fn name(&self) -> &'static str {
        "downloaded"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: show all downloaded plugins.
        println!("Command `downloaded` is not yet implemented.");
        Ok(())
    }
}
