// unmute.rs
// mute reversed.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct UnmuteCommand;

impl UnmuteCommand {
    /// `user` - the full username, e.g., HZFY@192.168.78.91, required, value_name: USER
    #[allow(unused_variables)]
	fn execute(&self, user: String) -> Result<()> {
		// TODO: mute reversed.
		println!("Command `unmute` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for UnmuteCommand {
	fn name(&self) -> &'static str {
		"unmute"
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
