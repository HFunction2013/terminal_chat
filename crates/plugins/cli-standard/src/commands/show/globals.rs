// globals.rs
// Show value of all global variables
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use libloading::Symbol;

pub struct GlobalsCommand;

impl GlobalsCommand {
    fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<fn(&safer_ffi::String)>;
        let get_all_options: Symbol<fn() -> safer_ffi::vec::Vec<GlobalOption>>;
        unsafe {
            print_content = lib
                .get::<fn(&safer_ffi::String)>(b"print_content")
                .expect("Failed to get `print_content`");
            get_all_options = lib
                .get::<fn() -> safer_ffi::vec::Vec<GlobalOption>>(b"get_all_options")
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
