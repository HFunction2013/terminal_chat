// op.rs
// give mod permission to a certain user.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
