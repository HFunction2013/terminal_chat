// call.rs
// Call an user
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

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
