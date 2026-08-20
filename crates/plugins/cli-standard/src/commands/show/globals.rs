// globals.rs
// Show value of all global variables
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_options);

        let ops = get_all_options();
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
