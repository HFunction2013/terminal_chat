// sessions.rs
// Multi-session manager
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct SessionsCommand;

impl CommandExecutor for SessionsCommand {
    fn name(&self) -> &'static str {
        "sessions"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Multi-session manager
        println!("Command `sessions` is not yet implemented.");
        Ok(())
    }
}
