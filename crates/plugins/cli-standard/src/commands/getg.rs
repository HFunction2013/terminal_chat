// getg.rs
// Get global options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct GetgCommand;

impl GetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    pub fn execute(&self, key: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_global_option);

        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let res = get_global_option(&key.clone().into(), Out::from(&mut buf));
        let val = ManuallyDrop::into_inner(buf);

        let found = res.code == 0 && matches!(val, TaggedOption::Some(_));
        if let TaggedOption::Some(v) = val {
            print_content(&format!("{key} => {v}").into());
        } else if !found {
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
