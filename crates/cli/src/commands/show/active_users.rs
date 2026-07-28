// active_users.rs
// Show active users in current frequency
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ActiveUsersCommand;

impl CommandExecutor for ActiveUsersCommand {
    fn name(&self) -> &'static str {
        "active_users"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show active users in current frequency
        println!("Command `active_users` is not yet implemented.");
        Ok(())
    }
}
