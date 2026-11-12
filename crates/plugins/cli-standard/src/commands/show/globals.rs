// globals.rs
// Show value of all global variables
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_options);

        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<GlobalOption>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_options(Out::from(&mut buf));
        let ops = ManuallyDrop::into_inner(buf);
        if ops.is_empty() {
            print_content(&"No options specified".into());
        } else {
            for opt in ops.iter() {
                print_content(&format!("Option {} => {}", opt.key, opt.value).into());
            }
        }
        Ok(())
    }
}

impl CommandExecutor for GlobalsCommand {
    fn name(&self) -> &'static str {
        "globals"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
