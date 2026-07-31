// variables.rs
// Show value of all variables
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct VariablesCommand;

impl CommandExecutor for VariablesCommand {
    fn name(&self) -> &'static str {
        "variables"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show value of all variables
        println!("Command `variables` is not yet implemented.");
        Ok(())
    }
}
