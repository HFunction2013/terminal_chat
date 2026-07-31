// msg.rs
// send message
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct MsgCommand;

impl CommandExecutor for MsgCommand {
    fn name(&self) -> &'static str {
        "msg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: send message
        println!("Command `msg` is not yet implemented.");
        Ok(())
    }
}
