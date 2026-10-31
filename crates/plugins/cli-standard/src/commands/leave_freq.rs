// leave_freq.rs
// Leave current frequency
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LeaveFreqCommand;

impl LeaveFreqCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let resp: protos::frequency::basic::LeaveFreqResponse =
            crate::connection::rpc("leave_freq", &protos::utils::empty::Empty {})?;
        if resp.result == protos::frequency::basic::LeaveResult::LeaveSuccess as i32 {
            crate::session::clear_current();
        }
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for LeaveFreqCommand {
    fn name(&self) -> &'static str {
        "leave_freq"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
