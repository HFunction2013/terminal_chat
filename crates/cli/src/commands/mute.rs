// mute.rs
// mute a certain user, use `set QUIET true` to mute everyboy except mods.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct MuteCommand;

impl MuteCommand {
    /// `user` - the full username, e.g., HZFY@192.168.78.91, required, value_name: USER
    #[allow(unused_variables)]
	fn execute(&self, user: String) -> Result<()> {
		// TODO: mute a certain user, use `set QUIET true` to mute everyboy except mods.
		println!("Command `mute` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for MuteCommand {
	fn name(&self) -> &'static str {
		"mute"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let user = matches
            .get_one::<String>("user")
            .ok_or_else(|| anyhow!("Missing required argument: user"))?
            .clone();
        self.execute(user)
	}
}
