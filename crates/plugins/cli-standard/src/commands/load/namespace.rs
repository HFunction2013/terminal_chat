// namespace.rs
// load namespace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct NamespaceCommand;

impl NamespaceCommand {
    pub fn execute(&self, namespace: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, is_namespace_active, mount_namespace);

        // 已挂载则幂等返回；未挂载（含仅加载代码）时挂载 namespace，把命令载入路由 / 命令表。
        let mut active: i32 = 0;
        let _ = is_namespace_active(&namespace.clone().into(), Out::from(&mut active));
        if active == 1 {
            print_content(
                &crate::tr("std.prompt.plugin.already_loaded")
                    .replace("{namespace}", &namespace)
                    .into(),
            );
            return Ok(());
        }

        let res = mount_namespace(&namespace.clone().into());
        let msg = res.message.to_string();
        if res.code == 0 {
            print_content(
                &crate::tr("std.prompt.plugin.ns_loaded")
                    .replace("{namespace}", &namespace)
                    .replace("{msg}", &msg)
                    .into(),
            );
        } else {
            print_content(
                &crate::tr("std.err.plugin.ns_not_found")
                    .replace("{namespace}", &namespace)
                    .replace("{msg}", &msg)
                    .replace("{code}", &res.code.to_string())
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
