// run.rs
// run a certain marco
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct RunCommand;

impl CommandExecutor for RunCommand {
    fn name(&self) -> &'static str {
        "run"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: run a certain marco
        println!("Command `run` is not yet implemented.");
        Ok(())
    }
}
