// updates.rs
// Updates
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct UpdatesCommand;

impl UpdatesCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&crate::tr("std.err.plugin.registry_disabled").into());
        Ok(())
    }
}

impl CommandExecutor for UpdatesCommand {
    fn name(&self) -> &'static str {
        "updates"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
