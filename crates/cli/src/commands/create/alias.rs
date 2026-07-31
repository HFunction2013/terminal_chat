// alias.rs
// Create alias
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
