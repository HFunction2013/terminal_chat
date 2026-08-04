// switch.rs
// Switch to another workspace.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct SwitchCommand;

impl SwitchCommand {
    /// `workspace_name` - workspace name., required, value_name: WORKSPACE_NAME
    #[allow(unused_variables)]
	fn execute(&self, workspace_name: String) -> Result<()> {
		// TODO: Switch to another workspace.
		println!("Command `switch` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for SwitchCommand {
	fn name(&self) -> &'static str {
		"switch"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let workspace_name = matches
            .get_one::<String>("workspace_name")
            .ok_or_else(|| anyhow!("Missing required argument: workspace_name"))?
            .clone();
        self.execute(workspace_name)
	}
}
