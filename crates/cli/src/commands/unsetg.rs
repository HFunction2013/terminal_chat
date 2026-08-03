// unsetg.rs
// Unset global variable.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct UnsetgCommand;

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    /// `force` - action without confirm.
    #[allow(unused_variables)]
	fn execute(&self, key: Option<String>, all: bool, force: bool) -> Result<()> {
		// TODO: Unset global variable.
		println!("Command `unsetg` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for UnsetgCommand {
	fn name(&self) -> &'static str {
		"unsetg"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .cloned();
        let all = matches.get_flag("all");
        let force = matches.get_flag("force");
        self.execute(key, all, force)
	}
}
