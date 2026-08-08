// macro.rs
// Delete macro
use crate::commands::CommandExecutor;
use crate::macros::{exists_macro, remove_macro};
use crate::print_content::print_content;
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct MacroCommand;

impl MacroCommand {
    /// `macro_name` - macro to delete, required, value_name: macro_NAME
    pub fn execute(&self, macro_name: String) -> Result<()> {
        if !exists_macro(&macro_name) {
            print_content(format!("Macro {macro_name} doesn't exist."));
            return Ok(());
        }
        remove_macro(&macro_name);
        print_content(format!("Macro {macro_name} deleted."));
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
