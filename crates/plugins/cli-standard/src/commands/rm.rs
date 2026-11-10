// rm.rs
// Remove a remote file or directory
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct RmCommand;

impl RmCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, path: String, recursive: bool) -> Result<()> {
        let resp: protos::remote::fs::RemoteRmResponse = crate::connection::rpc(
            "remote_rm",
            &protos::remote::fs::RemoteRmRequest { path, recursive },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for RmCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let path = matches
            .get_one::<String>("path")
            .ok_or_else(|| anyhow!("Missing required argument: path"))?
            .clone();
        let recursive = matches.get_flag("recursive");
        self.execute(path, recursive)
    }
}
