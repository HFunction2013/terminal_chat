// install.rs
// Install plugin
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
