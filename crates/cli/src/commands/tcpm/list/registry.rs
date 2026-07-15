// registry.rs
// Show all plugins from registry.
// Use this carefully.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

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
