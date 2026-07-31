// run.rs
// run a certain macro
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct RunCommand;

impl CommandExecutor for RunCommand {
    fn name(&self) -> &'static str {
        "run"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: run a certain macro
        println!("Command `run` is not yet implemented.");
        Ok(())
    }
}
