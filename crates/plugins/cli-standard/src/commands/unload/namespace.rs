// namespace.rs
// unload namespace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::PluginMetadata;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct NamespaceCommand;

impl NamespaceCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, namespace_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, get_all_plugins, unregister_plugin);
        // 命名空间即插件元数据的 `command_name`（与 `namespace list` 口径一致）；
        // 直接按已加载插件的命名空间匹配卸载，不再反查 namespaces.txt 分组清单。
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_plugins(Out::from(&mut buf));
        let plugins = ManuallyDrop::into_inner(buf);
        let mut unloaded = 0usize;
        for p in plugins.iter() {
            if p.command_name.to_string() == namespace_name {
                let _ = unregister_plugin(&p.name);
                unloaded += 1;
            }
        }
        if unloaded > 0 {
            print_content(
                &format!("[*] Namespace `{namespace_name}`: {unloaded} plugin(s) unloaded.").into(),
            );
        } else {
            print_content(
                &format!("[!] Namespace `{namespace_name}` not found among loaded plugins.").into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for NamespaceCommand {
    fn name(&self) -> &'static str {
        "namespace"
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
