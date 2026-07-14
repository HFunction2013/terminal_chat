// whoami.rs
// Get username.
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct WhoamiCommand;

impl CommandExecutor for WhoamiCommand {
    fn name(&self) -> &'static str {
        "whoami"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Get username.
        println!("Command `whoami` is not yet implemented.");
        Ok(())
    }
}
