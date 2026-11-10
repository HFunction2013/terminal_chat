// tree.rs
// Show remote directory tree
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct TreeCommand;

impl TreeCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, dir: Option<String>) -> Result<()> {
        let resp: protos::remote::fs::RemoteTreeResponse = crate::connection::rpc(
            "remote_tree",
            &protos::remote::fs::RemoteTreeRequest { dir: dir.unwrap_or_default() },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if !resp.success {
            print_content(&resp.text.into());
            return Ok(());
        }
        print_content(&resp.text.into());
        Ok(())
    }
}

impl CommandExecutor for TreeCommand {
    fn name(&self) -> &'static str {
        "tree"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let dir = matches.get_one::<String>("directory").cloned();
        self.execute(dir)
    }
}
