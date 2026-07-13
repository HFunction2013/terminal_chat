// file_info.rs
// Get remote file infomation.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

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
