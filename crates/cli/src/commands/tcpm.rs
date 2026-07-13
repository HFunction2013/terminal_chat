// tcpm.rs
// Terminal Chat Package Manager
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct TcpmCommand;

impl CommandExecutor for TcpmCommand {
    fn name(&self) -> &'static str {
        "tcpm"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Terminal Chat Package Manager
        println!("Command `tcpm` is not yet implemented.");
        Ok(())
    }
}
