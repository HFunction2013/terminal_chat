// disband.rs
// disband current frequency
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct DisbandCommand;

impl DisbandCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, force: bool) -> Result<()> {
        let resp: protos::frequency::mod_ops::DisbandResponse =
            crate::connection::rpc("disband_freq", &protos::frequency::mod_ops::DisbandRequest { freq: String::new(), force })?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for DisbandCommand {
    fn name(&self) -> &'static str {
        "disband"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let force = matches.get_flag("force");
        self.execute(force)
    }
}
