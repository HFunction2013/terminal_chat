// list.rs
// List all loaded plugins
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct ListCommand;

impl ListCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        crate::commands::show::plugins::PluginsCommand.execute()
    }
}

impl CommandExecutor for ListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
