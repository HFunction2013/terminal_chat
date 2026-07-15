// install.rs
// Install plugin
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct InstallCommand;

impl CommandExecutor for InstallCommand {
    fn name(&self) -> &'static str {
        "install"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Install plugin
        println!("Command `install` is not yet implemented.");
        Ok(())
    }
}
