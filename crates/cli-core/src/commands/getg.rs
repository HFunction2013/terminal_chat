// getg.rs
// Get global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use crate::global_settings::get_global_option;

pub struct GetgCommand;

impl GetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    #[allow(unused_variables)]
    fn execute(&self, key: String) -> Result<()> {
        let val = get_global_option(&key);
        if let Some(v) = val {
            println!("{key} => {v}");
        } else {
            println!("Key {key} doesn't exists");
            return Ok(());
        }
        Ok(())
    }
}

impl CommandExecutor for GetgCommand {
    fn name(&self) -> &'static str {
        "getg"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        self.execute(key)
    }
}
