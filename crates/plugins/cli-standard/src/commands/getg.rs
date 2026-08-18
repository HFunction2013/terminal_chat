// getg.rs
// Get global options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::{GetGlobalOptionFn, PrintContentFn};
use libloading::Symbol;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;

pub struct GetgCommand;

impl GetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    pub fn execute(&self, key: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<PrintContentFn>;
        let get_global_option: Symbol<GetGlobalOptionFn>;
        unsafe {
            print_content =
                lib.get::<PrintContentFn>(b"print_content").expect("Failed to get `print_content`");
            get_global_option = lib
                .get::<GetGlobalOptionFn>(b"get_global_option")
                .expect("Failed to get `get_global_option`");
        }

        let val = get_global_option(&key.clone().into());
        if let TaggedOption::Some(v) = val {
            print_content(&format!("{key} => {v}").into());
        } else {
            print_content(&format!("Key {key} doesn't exists").into());
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
