// globals.rs
// Show value of all global variables
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::{GetAllOptionsFn, PrintContentFn};
use libloading::Symbol;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<PrintContentFn>;
        let get_all_options: Symbol<GetAllOptionsFn>;
        unsafe {
            print_content =
                lib.get::<PrintContentFn>(b"print_content").expect("Failed to get `print_content`");
            get_all_options = lib
                .get::<GetAllOptionsFn>(b"get_all_options")
                .expect("Failed to get `get_all_options`");
        }

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
