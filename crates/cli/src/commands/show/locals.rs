// locals.rs
// Show value of all local variables
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct LocalsCommand;

impl CommandExecutor for LocalsCommand {
    fn name(&self) -> &'static str {
        "locals"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show value of all local variables
        println!("Command `locals` is not yet implemented.");
        Ok(())
    }
}
