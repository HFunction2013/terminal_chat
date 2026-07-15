// namespace.rs
// load namespace
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct NamespaceCommand;

impl CommandExecutor for NamespaceCommand {
    fn name(&self) -> &'static str {
        "namespace"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: load namespace
        println!("Command `namespace` is not yet implemented.");
        Ok(())
    }
}
