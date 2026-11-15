// list.rs
// List all loaded namespaces
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::PluginMetadata;
use safer_ffi::prelude::*;
use std::collections::BTreeSet;
use std::mem::ManuallyDrop;

pub struct ListCommand;

impl ListCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_plugins);

        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_plugins(Out::from(&mut buf));
        let plugins = ManuallyDrop::into_inner(buf);

        let namespaces: BTreeSet<String> =
            plugins.iter().map(|p| p.command_name.to_string()).collect();
        if namespaces.is_empty() {
            print_content(&"  (no namespaces loaded)".into());
            return Ok(());
        }
        for ns in namespaces {
            print_content(&format!("  {ns}").into());
        }
        Ok(())
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
