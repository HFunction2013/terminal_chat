// msg.rs
// send message
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
