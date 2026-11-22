// load.rs
// Load a namespace
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LoadCommand;

impl LoadCommand {
    /// `namespace` - Namespace to load, required, value_name: NAMESPACE
    #[allow(unused_variables)]
    pub fn execute(&self, namespace: String) -> Result<()> {
        crate::commands::load::namespace::NamespaceCommand.execute(namespace)
    }
}

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let namespace = matches
            .get_one::<String>("namespace")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "namespace")
                )
            })?
            .clone();
        self.execute(namespace)
    }
}
