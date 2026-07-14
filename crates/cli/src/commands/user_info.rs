// user_info.rs
// Show user information
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UserInfoCommand;

impl CommandExecutor for UserInfoCommand {
    fn name(&self) -> &'static str {
        "user_info"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show user information
        println!("Command `user_info` is not yet implemented.");
        Ok(())
    }
}
