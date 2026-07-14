// reply.rs
// Reply to the user who has just spoken quickly
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct ReplyCommand;

impl CommandExecutor for ReplyCommand {
    fn name(&self) -> &'static str {
        "reply"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Reply to the user who has just spoken quickly
        println!("Command `reply` is not yet implemented.");
        Ok(())
    }
}
