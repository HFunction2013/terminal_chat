// plugins.rs
// Show all loaded plugins (namespaces)
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::PluginMetadata;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct PluginsCommand;

impl PluginsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_plugins);

        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_plugins(Out::from(&mut buf));
        let plugins = ManuallyDrop::into_inner(buf);
        if plugins.is_empty() {
            print_content(&"  (no plugins loaded)".into());
            return Ok(());
        }
        for p in plugins.iter() {
            print_content(
                &format!("  {} (command: {})", p.name.to_string(), p.command_name.to_string())
                    .into(),
            );
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
