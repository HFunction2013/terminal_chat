// installed.rs
// show all installed plugins
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct InstalledCommand;

impl InstalledCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let installed = crate::local::read_installed();
        if installed.is_empty() {
            print_content(&crate::tr("std.prompt.plugin.no_installed").into());
            return Ok(());
        }
        for n in installed {
            print_content(&format!("  {}", n).into());
        }
        Ok(())
    }
}

impl CommandExecutor for InstalledCommand {
    fn name(&self) -> &'static str {
        "installed"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
