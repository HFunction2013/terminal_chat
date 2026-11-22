// plugin.rs
// load plugin
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct PluginCommand;

impl PluginCommand {
    pub fn execute(&self, plugin: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, load_plugin);
        let res = load_plugin(&plugin.clone().into());
        let msg = res.msg.into_rust().map(|s| s.to_string()).unwrap_or_default();
        if res.success == 1 {
            print_content(
                &crate::tr("std.prompt.plugin.loaded")
                    .replace("{plugin}", &plugin)
                    .replace("{msg}", &msg)
                    .into(),
            );
        } else {
            print_content(
                &crate::tr("std.err.plugin.load_failed")
                    .replace("{plugin}", &plugin)
                    .replace("{msg}", &msg)
                    .replace("{code}", &res.exit_code.to_string())
                    .into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for PluginCommand {
    fn name(&self) -> &'static str {
        "plugin"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin"))
            })?
            .clone();
        self.execute(plugin)
    }
}
