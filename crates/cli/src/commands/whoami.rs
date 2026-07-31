// whoami.rs
// Get username.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
