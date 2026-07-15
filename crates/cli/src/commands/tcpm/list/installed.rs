// installed.rs
// show all installed plugins.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct InstalledCommand;

impl CommandExecutor for InstalledCommand {
    fn name(&self) -> &'static str {
        "installed"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: show all installed plugins.
        println!("Command `installed` is not yet implemented.");
        Ok(())
    }
}
