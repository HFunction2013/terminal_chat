// uninstall.rs
// Uninstall plugin
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct UninstallCommand;

impl UninstallCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, plugin: String, delete_local: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, unregister_plugin);
        let _ = unregister_plugin(&plugin.clone().into());
        let mut installed = crate::local::read_installed();
        installed.retain(|n| n != &plugin);
        crate::local::write_installed(&installed)?;
        if delete_local {
            for f in
                std::fs::read_dir(crate::local::plugins_dir()).ok().into_iter().flatten().flatten()
            {
                if f.file_name().to_string_lossy().starts_with(&plugin) {
                    let _ = std::fs::remove_file(f.path());
                }
            }
            print_content(
                &crate::tr("std.prompt.plugin.uninstalled_removed")
                    .replace("{plugin}", &plugin)
                    .into(),
            );
        } else {
            print_content(
                &crate::tr("std.prompt.plugin.uninstalled_kept")
                    .replace("{plugin}", &plugin)
                    .into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for UninstallCommand {
    fn name(&self) -> &'static str {
        "uninstall"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin"))
            })?
            .clone();
        let delete_local = matches.get_flag("delete_local");
        self.execute(plugin, delete_local)
    }
}
