// show.rs
// Show
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ShowCommand;

impl ShowCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, plugin: String, local: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&crate::tr("std.err.plugin.registry_disabled").into());
        Ok(())
    }
}

impl CommandExecutor for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin"))
            })?
            .clone();
        let local = matches.get_flag("local");
        self.execute(plugin, local)
    }
}
