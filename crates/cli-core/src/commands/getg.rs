// getg.rs
// Get global options
use crate::commands::CommandExecutor;
use crate::global_settings::get_global_option;
use crate::print_content::print_content;
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct GetgCommand;

impl GetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    pub fn execute(&self, key: String) -> Result<()> {
        let val = unsafe { get_global_option(key.clone().into()) };
        unsafe {
            if let Some(v) = val {
                print_content(format!("{key} => {v}").into());
            } else {
                print_content(format!("Key {key} doesn't exists").into());
            }
        }
        Ok(())
    }
}

impl CommandExecutor for GetgCommand {
    fn name(&self) -> &'static str {
        "getg"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        self.execute(key)
    }
}
