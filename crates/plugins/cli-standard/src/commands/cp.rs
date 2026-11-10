// cp.rs
// Copy a remote file/directory
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct CpCommand;

impl CpCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, from: String, to: String) -> Result<()> {
        let resp: protos::remote::fs::RemoteCpResponse = crate::connection::rpc(
            "remote_cp",
            &protos::remote::fs::RemoteCpRequest { from, to },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for CpCommand {
    fn name(&self) -> &'static str {
        "cp"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let from = matches
            .get_one::<String>("from")
            .ok_or_else(|| anyhow!("Missing required argument: from"))?
            .clone();
        let to = matches
            .get_one::<String>("to")
            .ok_or_else(|| anyhow!("Missing required argument: to"))?
            .clone();
        self.execute(from, to)
    }
}
