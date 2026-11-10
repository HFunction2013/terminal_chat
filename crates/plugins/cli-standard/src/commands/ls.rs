// ls.rs
// List files in a remote directory (default: current directory)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LsCommand;

impl LsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, dir: Option<String>) -> Result<()> {
        let resp: protos::remote::fs::LsResponse = crate::connection::rpc(
            "remote_ls",
            &protos::remote::fs::LsRequest { dir: dir.unwrap_or_default() },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if !resp.success {
            print_content(&resp.message.into());
            return Ok(());
        }
        print_content(&format!("{}:", resp.directory).into());
        for e in resp.entries {
            print_content(&format!("  {}{}",
                if e.is_dir { "d" } else { "f" }, e.name).into());
        }
        Ok(())
    }
}

impl CommandExecutor for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let dir = matches.get_one::<String>("directory").cloned();
        self.execute(dir)
    }
}
