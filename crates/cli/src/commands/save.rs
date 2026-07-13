// save.rs
// 
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct SaveCommand;

impl CommandExecutor for SaveCommand {
    fn name(&self) -> &'static str {
        "save"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: 
        println!("Command `save` is not yet implemented.");
        Ok(())
    }
}
