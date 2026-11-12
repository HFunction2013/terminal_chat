// unsetg.rs
// Unset global variable.
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct UnsetgCommand;

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    pub fn execute(&self, key: Option<String>, all: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            clear_all_options,
            remove_global_option
        );

        if all {
            let _res = clear_all_options();
            print_content(&"All global variables have been cleared.".into());
            return Ok(());
        }

        let key = key.ok_or_else(|| anyhow!("Missing required argument: key"))?;

        let key_for_display = key.clone();
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let _res = remove_global_option(&key.into(), Out::from(&mut buf));
        let val = ManuallyDrop::into_inner(buf);
        if let TaggedOption::Some(_) = val {
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
