// macro.rs
// Delete macro
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use crate::LIB;
use libloading::Symbol;
use safer_ffi::prelude::*;

pub struct MacroCommand;

impl MacroCommand {
    /// `macro_name` - macro to delete, required, value_name: macro_NAME
    pub fn execute(&self, macro_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<fn(&safer_ffi::String)>;
        let remove_macro: Symbol<fn(&repr_c::String) -> TaggedOption<repr_c::String>>;
        unsafe {
            print_content = lib
                .get::<fn(&safer_ffi::String)>(b"print_content")
                .expect("Failed to get `print_content`");
            remove_macro = lib
                .get::<fn(&repr_c::String) -> TaggedOption<repr_c::String>>(b"remove_macro")
                .expect("Failed to get `remove_macro`");
        }

        if remove_macro(&macro_name.clone().into()).into_rust().is_some() {
            print_content(&format!("Macro {macro_name} deleted.").into());
        } else {
            print_content(&format!("Macro {macro_name} doesn't exist.").into());
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
