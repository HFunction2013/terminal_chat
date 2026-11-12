// macro.rs
// Delete macro
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct MacroCommand;

impl MacroCommand {
    /// `macro_name` - macro to delete, required, value_name: macro_NAME
    pub fn execute(&self, macro_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, remove_macro);

        let name_for_display = macro_name.clone();
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let _res = remove_macro(&macro_name.into(), Out::from(&mut buf));
        let removed = !matches!(ManuallyDrop::into_inner(buf), TaggedOption::None);
        if removed {
            print_content(&format!("Macro {name_for_display} deleted.").into());
        } else {
            print_content(&format!("Macro {name_for_display} doesn't exist.").into());
        }
        Ok(())
    }
}

impl CommandExecutor for MacroCommand {
    fn name(&self) -> &'static str {
        "macro"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let macro_name = matches
            .get_one::<String>("macro_name")
            .ok_or_else(|| anyhow!("Missing required argument: macro_name"))?
            .clone();
        self.execute(macro_name)
    }
}
