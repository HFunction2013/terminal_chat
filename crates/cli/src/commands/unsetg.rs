// unsetg.rs
// Unset global variable.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core::{
    VOID,
    global_settings::{clear_all_options, exists_global_option, remove_global_option},
};
use std::io;

pub struct UnsetgCommand;

impl UnsetgCommand {
    fn confirm(force: bool) -> bool {
        if !force {
            println!("Sure to proceed? This cannot be undone.(Y/N) ");
            let mut input = String::new();
            io::stdin().read_line(&mut input).expect("fail to confirm");
            let lower = input.trim().to_lowercase();
            lower == "y" || lower == "yes"
        } else {
            true
        }
    }
}

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    /// `force` - action without confirm.
    #[allow(unused_variables)]
    fn execute(&self, key: Option<String>, all: bool, force: bool) -> Result<()> {
        if all {
            if Self::confirm(force) {
                clear_all_options(VOID);
                println!("All global variables have been cleared.");
            } else {
                println!("Operation cancelled.");
            }
            return Ok(());
        }

        let key = key.ok_or_else(|| anyhow!("Missing required argument: key"))?;
        
		if !exists_global_option(&key) {
            println!("Key {key} doesn't exists");
            return Ok(());
        }
        if Self::confirm(force) {
            let res = remove_global_option(&key);
            match res {
                #[allow(non_snake_case)]
                None => println!("Key {key} doesn't exists"),
                _ => println!("Global variable '{key}' has been removed."),
            }
        } else {
            println!("Operation cancelled.");
        }

        Ok(())
    }
}

impl CommandExecutor for UnsetgCommand {
    fn name(&self) -> &'static str {
        "unsetg"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches.get_one::<String>("key").cloned();
        let all = matches.get_flag("all");
        let force = matches.get_flag("force");
        self.execute(key, all, force)
    }
}
