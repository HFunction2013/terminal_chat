// globals.rs
// Show value of all global variables
use crate::{
    commands::CommandExecutor, global_settings::get_all_options, print_content::print_content,
};
use anyhow::Result;
use clap::ArgMatches;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let ops = unsafe { get_all_options() };
        if ops.is_empty() {
            unsafe { print_content("No options specified".into()) };
        } else {
            for opt in ops.iter() {
                unsafe {
                    print_content(format!("Option {} => {}", opt.key, opt.value).into());
                }
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
