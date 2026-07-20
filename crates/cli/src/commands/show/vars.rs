// vars.rs
// Show value of all variables
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct VarsCommand;

impl CommandExecutor for VarsCommand {
    fn name(&self) -> &'static str {
        "vars"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show value of all variables
        println!("Command `vars` is not yet implemented.");
        Ok(())
    }
}
