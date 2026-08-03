// msg.rs
// send message
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct MsgCommand;

impl MsgCommand {
    /// `message` - Your message, send #EDITOR# to open editor, required, value_name: MSG
    /// `users` - The users, default everybody., value_name: USERS
    #[allow(unused_variables)]
	fn execute(&self, message: String, users: Vec<String>) -> Result<()> {
		// TODO: send message
		println!("Command `msg` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for MsgCommand {
	fn name(&self) -> &'static str {
		"msg"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let message = matches
            .get_one::<String>("message")
            .ok_or_else(|| anyhow!("Missing required argument: message"))?
            .clone();
        let users = matches
            .get_many::<String>("users")
            .unwrap_or_default()
            .map(|s| s.clone())
            .collect::<Vec<_>>();
        self.execute(message, users)
	}
}
