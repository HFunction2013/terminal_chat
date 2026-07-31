// namespace.rs
// load namespace
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
