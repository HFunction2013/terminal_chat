// unsetg.rs
// Unset global variable.
use crate::commands::CommandExecutor;
use crate::{
    VOID,
    global_settings::{clear_all_options, remove_global_option},
    print_content::print_content,
};
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UnsetgCommand;

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    pub fn execute(&self, key: Option<String>, all: bool) -> Result<()> {
        if all {
            clear_all_options(VOID);
            print_content("All global variables have been cleared.");
            return Ok(());
        }

        let key = key.ok_or_else(|| anyhow!("Missing required argument: key"))?;

        if remove_global_option(&key).is_some() {
            print_content(format!("Global variable '{key}' has been removed."));
        } else {
            print_content(format!("Key {key} doesn't exist."));
        }

        Ok(())
    }
}

impl CommandExecutor for UnsetgCommand {
    fn name(&self) -> &'static str {
        "unsetg"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches.get_one::<String>("key").cloned();
        let all = matches.get_flag("all");
        self.execute(key, all)
    }
}
