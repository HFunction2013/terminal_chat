// search.rs
// Search
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct SearchCommand;

impl SearchCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, plugin_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&crate::tr("std.err.plugin.registry_disabled").into());
        Ok(())
    }
}

impl CommandExecutor for SearchCommand {
    fn name(&self) -> &'static str {
        "search"
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
