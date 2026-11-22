// switch.rs
// Switch to another workspace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct SwitchCommand;

impl SwitchCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, workspace_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, reload_aliases, reload_macros);
        let dir = crate::local::workspace_path(&workspace_name);
        if !dir.is_dir() {
            // default 是内建 workspace，始终可进入（首次使用时创建）。
            if workspace_name == "default" {
                std::fs::create_dir_all(&dir)?;
            } else {
                print_content(
                    &crate::tr("std.err.workspace.not_found")
                        .replace("{name}", &workspace_name)
                        .into(),
                );
                return Ok(());
            }
        }
        crate::local::set_current_workspace(&workspace_name)?;
        // 独立环境：本地 cwd 切到该 workspace 的 home，别名/宏按 workspace 重新加载。
        let home = crate::local::workspace_home(&workspace_name);
        std::fs::create_dir_all(&home)?;
        std::env::set_current_dir(&home)
            .map_err(|e| anyhow!("cannot switch into `{}`: {e}", home.display()))?;
        let _ = reload_aliases();
        let _ = reload_macros();
        print_content(
            &crate::tr("std.prompt.workspace.switched")
                .replace("{name}", &workspace_name)
                .replace("{home}", &home.display().to_string())
                .into(),
        );
        Ok(())
    }
}

impl CommandExecutor for SwitchCommand {
    fn name(&self) -> &'static str {
        "switch"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let workspace_name = matches
            .get_one::<String>("workspace_name")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "workspace_name")
                )
            })?
            .clone();
        self.execute(workspace_name)
    }
}
