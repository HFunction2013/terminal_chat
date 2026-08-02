// getg.rs
// Get global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core::global_settings::get_global_option;

pub struct GetgCommand;

impl CommandExecutor for GetgCommand {
    fn name(&self) -> &'static str {
        "getg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        let key = _matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?;
        let val = get_global_option(key);
        if let Some(v) = val {
            println!("{key} => {v}");
        } else {
            println!("Key {key} doesn't exists");
            return Ok(());
        }
        Ok(())
    }
}
