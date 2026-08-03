// upload.rs
// Upload file or folder to server.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct UploadCommand;

impl UploadCommand {
    /// `local_path` - Local path to upload., required, value_name: LOCAL_PATH
    /// `remote_path` - Remote path to upload., value_name: REMOTE_PATH
    #[allow(unused_variables)]
	fn execute(&self, local_path: String, remote_path: Option<String>) -> Result<()> {
		// TODO: Upload file or folder to server.
		println!("Command `upload` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for UploadCommand {
	fn name(&self) -> &'static str {
		"upload"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let local_path = matches
            .get_one::<String>("local_path")
            .ok_or_else(|| anyhow!("Missing required argument: local_path"))?
            .clone();
        let remote_path = matches
            .get_one::<String>("remote_path")
            .cloned();
        self.execute(local_path, remote_path)
	}
}
