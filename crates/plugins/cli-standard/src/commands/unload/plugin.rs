// plugin.rs
// unload plugin
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct PluginCommand;

impl PluginCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, plugin_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        cli_core_macros::load_core_symbols!(lib, unregister_plugin);
        let _ = unregister_plugin(&plugin_name.clone().into());
        print_content(
            &crate::tr("std.prompt.plugin.unloaded").replace("{plugin_name}", &plugin_name).into(),
        );
        Ok(())
    }
}

impl CommandExecutor for PluginCommand {
    fn name(&self) -> &'static str {
        "plugin"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin_name = matches
            .get_one::<String>("plugin_name")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin_name")
                )
            })?
            .clone();
        self.execute(plugin_name)
    }
}
