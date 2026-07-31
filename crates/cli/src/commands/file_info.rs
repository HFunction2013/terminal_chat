// file_info.rs
// Get remote file infomation.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct FileInfoCommand;

impl CommandExecutor for FileInfoCommand {
    fn name(&self) -> &'static str {
        "file_info"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get remote file infomation.
        println!("Command `file_info` is not yet implemented.");
        Ok(())
    }
}
