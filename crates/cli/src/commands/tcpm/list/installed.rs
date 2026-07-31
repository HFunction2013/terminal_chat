// installed.rs
// show all installed plugins.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
