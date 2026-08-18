// macro.rs
// Create macro
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::MacroDef;
use cli_core_types::{PrintContentFn, SetMacroFn};
use libloading::Symbol;
use std::env;
use std::fs;

pub struct MacroCommand;

impl MacroCommand {
    /// `macro_name` - macro name, required, value_name: MACRO_NAME
    /// `macro_body` - macro body, default: open editor to get input
    pub fn execute(&self, macro_name: String, macro_body: Option<String>) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<PrintContentFn>;
        let set_macro: Symbol<SetMacroFn>;
        unsafe {
            print_content =
                lib.get::<PrintContentFn>(b"print_content").expect("Failed to get `print_content`");
            set_macro = lib.get::<SetMacroFn>(b"set_macro").expect("Failed to get `set_macro`");
        }

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
            print_content(&"Macro body not specified. Cannot create macro.".into());
            return Ok(());
        }

        let macro_def = MacroDef { name: macro_name.clone().into(), code: code.into() };

        set_macro(&macro_def);
        print_content(&format!("Macro {macro_name} created.").into());
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
