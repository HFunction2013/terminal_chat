// macro.rs
// Create macro
use crate::commands::CommandExecutor;
use crate::macros::MacroDef;
use crate::macros::set_macro;
use crate::print_content::print_content;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::env;
use std::fs;

pub struct MacroCommand;

impl MacroCommand {
    /// `macro_name` - macro name, required, value_name: MACRO_NAME
    /// `macro_body` - macro body, default: open editor to get input
    pub fn execute(&self, macro_name: String, macro_body: Option<String>) -> Result<()> {
        let mut path = env::current_exe()?;
        path.pop();
        path.push("CREATE_EDITMACRO");
        fs::write(&path, "")?;

        let code = if let Some(c) = macro_body {
            c
        } else {
            crate::commands::editor::EditorCommand
                .execute(Some(path.to_string_lossy().to_string()), None)?;

            fs::read_to_string(&path)?
        };
        if code.is_empty() {
            print_content("Macro body not specified. Cannot create macro.".to_string());
            return Ok(());
        }
        set_macro(MacroDef { name: macro_name.clone(), code });
        print_content(format!("Macro {macro_name} created."));
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
        let macro_body = matches.get_one::<String>("macro_body").cloned();
        self.execute(macro_name, macro_body)
    }
}
