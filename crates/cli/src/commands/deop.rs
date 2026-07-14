// deop.rs
// take mod permission from a certain user, noone can deop creator.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
