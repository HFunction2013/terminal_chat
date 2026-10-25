// download.rs
// Download file or folder to server
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct DownloadCommand;

impl DownloadCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, remote_path: String, local_path: Option<String>) -> Result<()> {
        let resp: protos::remote::fs::DownloadResponse = crate::connection::rpc(
            "download",
            &protos::remote::fs::DownloadRequest {
                remote_path: remote_path.clone(),
                local_path: local_path.clone().unwrap_or_default(),
            },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if resp.found {
            let dest = local_path.unwrap_or_else(|| {
                remote_path
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("download.bin")
                    .to_string()
            });
            std::fs::write(&dest, &resp.content)
                .map_err(|e| anyhow!("cannot write `{dest}`: {e}"))?;
            print_content(&format!("{} bytes downloaded to {dest}", resp.content.len()).into());
        } else {
            print_content(&format!("`{remote_path}` not found").into());
        }
        Ok(())
    }
}

impl CommandExecutor for DownloadCommand {
    fn name(&self) -> &'static str {
        "download"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let remote_path = matches
            .get_one::<String>("remote_path")
            .ok_or_else(|| anyhow!("Missing required argument: remote_path"))?
            .clone();
        let local_path = matches.get_one::<String>("local_path").cloned();
        self.execute(remote_path, local_path)
    }
}
