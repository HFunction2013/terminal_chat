// sessions.rs
// Multi-session manager
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
