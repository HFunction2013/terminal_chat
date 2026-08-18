// run.rs
// run a certain macro
use crate::LIB;
use crate::commands::CommandExecutor;
use ::safer_ffi::prelude::*;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::{GetMacroFn, PrintContentFn, RunCommandFn};
use libloading::Symbol;
use safer_ffi::option::TaggedOption;
pub struct RunCommand;

impl RunCommand {
    /// `macro_name` - macro name, required, value_name: macro_NAME
    pub fn execute(&self, macro_name: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<PrintContentFn>;
        let get_macro: Symbol<GetMacroFn>;
        let run_command: Symbol<RunCommandFn>;
        unsafe {
            print_content =
                lib.get::<PrintContentFn>(b"print_content").expect("Failed to get `print_content`");
            get_macro = lib.get::<GetMacroFn>(b"get_macro").expect("Failed to get `get_macro`");
            run_command =
                lib.get::<RunCommandFn>(b"run_command").expect("Failed to get `run_command`");
        }
        let macro_name_display = macro_name.clone();
        if let TaggedOption::Some(code) = get_macro(&macro_name.into()) {
            let lines: Vec<&str> = code.split('\n').collect();

            for line in lines.iter() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let args = match shell_words::split(line) {
                    Ok(args) => args,
                    Err(e) => {
                        eprintln!("Parse error: {e}");
                        continue;
                    }
                };
                let args_repr_c: Vec<safer_ffi::String> =
                    args.into_iter().map(|s| s.into()).collect();
                let args_ffi: safer_ffi::Vec<safer_ffi::String> = args_repr_c.into();
                let result = run_command(&args_ffi);
                if result.code != 0 {
                    eprintln!("Command failed: {}", result.message);
                }
            }
        } else {
            print_content(&format!("Macro {macro_name_display} doesn't exists.").into());
        }
        Ok(())
    }
}

impl CommandExecutor for RunCommand {
    fn name(&self) -> &'static str {
        "run"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let macro_name = matches
            .get_one::<String>("macro_name")
            .ok_or_else(|| anyhow!("Missing required argument: macro_name"))?
            .clone();
        self.execute(macro_name)
    }
}
