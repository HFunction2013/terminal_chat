// downloaded.rs
// show all downloaded plugins.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
