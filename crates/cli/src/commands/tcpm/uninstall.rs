// uninstall.rs
// Uninstall plugin
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
