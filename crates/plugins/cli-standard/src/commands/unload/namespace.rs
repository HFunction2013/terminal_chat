// namespace.rs
// unload namespace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct NamespaceCommand;

impl NamespaceCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, namespace_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, is_namespace_active, detach_namespace);

        let mut active: i32 = 0;
        let _ = is_namespace_active(&namespace_name.clone().into(), Out::from(&mut active));
        if active != 1 {
            print_content(
                &crate::tr("std.err.plugin.ns_not_loaded")
                    .replace("{namespace_name}", &namespace_name)
                    .into(),
            );
            return Ok(());
        }

        // 仅摘除 namespace 路由与活跃标记；插件代码、库句柄与已注册归属项保留。
        let res = detach_namespace(&namespace_name.clone().into());
        if res.code == 0 {
            print_content(
                &crate::tr("std.prompt.plugin.ns_unloaded")
                    .replace("{namespace_name}", &namespace_name)
                    .replace("{count}", "1")
                    .into(),
            );
        } else {
            print_content(
                &crate::tr("std.err.plugin.ns_not_loaded")
                    .replace("{namespace_name}", &namespace_name)
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
        let namespace_name = matches
            .get_one::<String>("namespace_name")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "namespace_name")
                )
            })?
            .clone();
        self.execute(namespace_name)
    }
}
