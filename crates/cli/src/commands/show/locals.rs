// locals.rs
// Show value of all local variables
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
