// unload.rs
// Unload a namespace
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UnloadCommand;

impl UnloadCommand {
    /// `namespace_name` - Namespace to unload, required, value_name: NAMESPACE_NAME
    #[allow(unused_variables)]
    pub fn execute(&self, namespace_name: String) -> Result<()> {
        crate::commands::unload::namespace::NamespaceCommand.execute(namespace_name)
    }
}

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let namespace_name = matches
            .get_one::<String>("namespace_name")
            .ok_or_else(|| anyhow!("Missing required argument: namespace_name"))?
            .clone();
        self.execute(namespace_name)
    }
}
