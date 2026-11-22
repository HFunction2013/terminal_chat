// namespace.rs
// load namespace
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
    pub fn execute(&self, namespace: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, get_all_plugins, load_plugin);

        // 命名空间即插件元数据的 `command_name`（与 `namespace list` / `unload namespace`
        // 口径一致）；先在已加载集合中按 command_name 幂等匹配，不再反查 namespaces.txt
        // 分组清单。
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = get_all_plugins(Out::from(&mut buf));
        let loaded = ManuallyDrop::into_inner(buf);
        if loaded.iter().any(|p| p.command_name.to_string() == namespace) {
            print_content(&format!("[*] Namespace `{namespace}` is already loaded.").into());
            return Ok(());
        }

        // 未加载：按该名称走既有插件库查找与加载路径（与 `load plugin` 同一发现路径）。
        // 注意：若某插件的 command_name 与库名不同且尚未加载，需用 `load plugin <库名>`。
        let res = load_plugin(&namespace.clone().into());
        if res.success == 1 {
            print_content(
                &format!(
                    "[*] Namespace `{namespace}` loaded: {}",
                    res.msg.into_rust().map(|s| s.to_string()).unwrap_or_default()
                )
                .into(),
            );
        } else {
            print_content(
                &format!(
                    "[!] Namespace `{namespace}` not found among loaded plugins or installed libraries: {} (exit {})",
                    res.msg.into_rust().map(|s| s.to_string()).unwrap_or_default(),
                    res.exit_code
                )
                .into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for NamespaceCommand {
    fn name(&self) -> &'static str {
        "namespace"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let namespace = matches
            .get_one::<String>("namespace")
            .ok_or_else(|| anyhow!("Missing required argument: namespace"))?
            .clone();
        self.execute(namespace)
    }
}
