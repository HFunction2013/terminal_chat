// unsetg.rs
// Unset global variable.
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use libloading::Symbol;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;

pub struct UnsetgCommand;

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    pub fn execute(&self, key: Option<String>, all: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<fn(&safer_ffi::String)>;
        let remove_global_option: Symbol<fn(&safer_ffi::String) -> TaggedOption<safer_ffi::String>>;
        let clear_all_options: Symbol<fn()>;
        unsafe {
            print_content = lib
                .get::<fn(&safer_ffi::String)>(b"print_content")
                .expect("Failed to get `print_content`");
            remove_global_option = lib
                .get::<fn(&safer_ffi::String) -> TaggedOption<safer_ffi::String>>(b"remove_global_option")
                .expect("Failed to get `remove_global_option`");
            clear_all_options =
                lib.get::<fn()>(b"clear_all_options").expect("Failed to get `clear_all_options`");
        }

        if all {
            clear_all_options();
            print_content(&"All global variables have been cleared.".into());
            return Ok(());
        }

        let key = key.ok_or_else(|| anyhow!("Missing required argument: key"))?;

        let key_for_display = key.clone();
        if remove_global_option(&key.into()).into_rust().is_some() {
            print_content(&format!("Global variable '{key_for_display}' has been removed.").into());
        } else {
            print_content(&format!("Key {key_for_display} doesn't exist.").into());
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
