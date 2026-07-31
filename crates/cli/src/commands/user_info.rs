// user_info.rs
// Show user information
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
