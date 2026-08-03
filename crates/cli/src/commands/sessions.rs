// sessions.rs
// Multi-session manager
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct SessionsCommand;

impl SessionsCommand {
    /// `list` - List all running sessions
    /// `interact` - Switch to target session, value_name: SESSION_ID
    /// `kill` - Close and destroy target session, value_name: SESSION_ID
    #[allow(unused_variables)]
	fn execute(&self, list: bool, interact: String, kill: String) -> Result<()> {
		// TODO: Multi-session manager
		println!("Command `sessions` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for SessionsCommand {
	fn name(&self) -> &'static str {
		"sessions"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let list = matches.get_flag("list");
        let interact = matches
            .get_one::<String>("interact")
            .ok_or_else(|| anyhow!("Missing required argument: interact"))?
            .clone();
        let kill = matches
            .get_one::<String>("kill")
            .ok_or_else(|| anyhow!("Missing required argument: kill"))?
            .clone();
        self.execute(list, interact, kill)
	}
}
