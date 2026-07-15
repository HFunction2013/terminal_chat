// alias.rs
// Create alias
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct AliasCommand;

impl CommandExecutor for AliasCommand {
    fn name(&self) -> &'static str {
        "alias"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Create alias
        println!("Command `alias` is not yet implemented.");
        Ok(())
    }
}
