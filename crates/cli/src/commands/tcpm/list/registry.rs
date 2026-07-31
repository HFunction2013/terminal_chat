// registry.rs
// Show all plugins from registry.
// Use this carefully.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct RegistryCommand;

impl CommandExecutor for RegistryCommand {
    fn name(&self) -> &'static str {
        "registry"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Show all plugins from registry.
        // TODO: Use this carefully.
        println!("Command `registry` is not yet implemented.");
        Ok(())
    }
}
