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
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            get_all_plugins,
            is_namespace_active
        );

        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_plugins(Out::from(&mut buf));
        let plugins = ManuallyDrop::into_inner(buf);
        if plugins.is_empty() {
            print_content(&crate::tr("std.prompt.plugin.no_loaded").into());
            return Ok(());
        }
        for p in plugins.iter() {
            // 已挂载 namespace 的条目在命名空间后追加 `*`；仅加载代码未挂载的不加。
            let mut active: i32 = 0;
            let _ = is_namespace_active(&p.name, Out::from(&mut active));
            let ns = p.command_name.to_string();
            let ns_label = if active == 1 { format!("{ns}*") } else { ns };
            print_content(&format!("  {} (command: {})", p.name, ns_label).into());
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
