// globals.rs
// Show value of all global variables
use crate::{
    VOID, commands::CommandExecutor, global_settings::get_all_options, print_content::print_content,
};
use anyhow::Result;
use clap::ArgMatches;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let ops = get_all_options(VOID);
        if ops.is_empty() {
            print_content("No options specified");
        } else {
            for (key, value) in &ops {
                print_content(format!("Option {key} => {value}"));
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
