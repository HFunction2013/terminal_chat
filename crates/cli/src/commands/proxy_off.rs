// proxy_off.rs
// Turn off system proxy
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct ProxyOffCommand;

impl CommandExecutor for ProxyOffCommand {
    fn name(&self) -> &'static str {
        "proxy_off"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        // TODO: Turn off system proxy
        println!("Command `proxy_off` is not yet implemented.");
        Ok(())
    }
}
