// mv.rs
// Move or rename a remote file/directory
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct MvCommand;

impl MvCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, from: String, to: String) -> Result<()> {
        let resp: protos::remote::fs::RemoteMvResponse = crate::connection::rpc(
            "remote_mv",
            &protos::remote::fs::RemoteMvRequest { from, to },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for MvCommand {
    fn name(&self) -> &'static str {
        "mv"
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
