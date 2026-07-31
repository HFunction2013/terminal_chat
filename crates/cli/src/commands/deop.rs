// deop.rs
// take mod permission from a certain user, noone can deop creator.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct DeopCommand;

impl CommandExecutor for DeopCommand {
    fn name(&self) -> &'static str {
        "deop"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: take mod permission from a certain user, noone can deop creator.
        println!("Command `deop` is not yet implemented.");
        Ok(())
    }
}
