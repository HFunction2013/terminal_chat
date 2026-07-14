// op.rs
// give mod permission to a certain user.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct OpCommand;

impl CommandExecutor for OpCommand {
    fn name(&self) -> &'static str {
        "op"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: give mod permission to a certain user.
        println!("Command `op` is not yet implemented.");
        Ok(())
    }
}
