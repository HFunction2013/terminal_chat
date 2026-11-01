// plugins.rs
// Show all loaded plugins (namespaces)
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct PluginsCommand;

impl PluginsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_plugins);
        let plugins = get_all_plugins();
        if plugins.is_empty() {
            print_content(&"  (no plugins loaded)".into());
            return Ok(());
        }
        for p in plugins.iter() {
            print_content(&format!("  {} (command: {})", p.name.to_string(), p.command_name.to_string()).into());
        }
        Ok(())
    }
}

impl CommandExecutor for PluginsCommand {
    fn name(&self) -> &'static str {
        "plugins"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
