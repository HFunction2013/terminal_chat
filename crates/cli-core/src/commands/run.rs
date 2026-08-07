// run.rs
// run a certain macro
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use crate::macros::get_macro;
use crate::print_content::print_content;
use crate::run_commands::run_command;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
pub struct RunCommand;

impl RunCommand {
    /// `macro_name` - macro name, required, value_name: macro_NAME
    #[allow(unused_variables)]
    pub fn execute(&self, macro_name: String) -> Result<()> {
        if let Some(code) = get_macro(&macro_name) {
            let lines: Vec<&str> = code.split('\n').collect();

            for (i, line) in lines.iter().enumerate() {
                let args = match shell_words::split(line) {
                    Ok(args) => args,
                    Err(e) => {
                        eprintln!("Parse error: {e}");
                        continue;
                    }
                };
                match run_command(args) {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("Command failed: {err}");
                    }
                };
            }
        } else {
            print_content(format!("Macro {macro_name} doesn't exists."));
        }
        Ok(())
    }
}

impl CommandExecutor for RunCommand {
    fn name(&self) -> &'static str {
        "run"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let macro_name = matches
            .get_one::<String>("macro_name")
            .ok_or_else(|| anyhow!("Missing required argument: macro_name"))?
            .clone();
        self.execute(macro_name)
    }
}
