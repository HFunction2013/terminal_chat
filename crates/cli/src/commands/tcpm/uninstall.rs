// uninstall.rs
// Uninstall plugin
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct UninstallCommand;

impl CommandExecutor for UninstallCommand {
    fn name(&self) -> &'static str {
        "uninstall"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Uninstall plugin
        println!("Command `uninstall` is not yet implemented.");
        Ok(())
    }
}
