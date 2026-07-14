// call.rs
// Call an user
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct CallCommand;

impl CommandExecutor for CallCommand {
    fn name(&self) -> &'static str {
        "call"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Call an user
        println!("Command `call` is not yet implemented.");
        Ok(())
    }
}
