// foreground.rs
// Switch back to the most recently backgrounded session (freq)
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ForegroundCommand;

impl ForegroundCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        // 恢复最近挂起的 freq：重新 join_freq 并设为当前会话。
        if let Some(freq) = crate::session::foreground_next() {
            let resp: protos::frequency::basic::JoinFreqResponse = crate::connection::rpc(
                "join_freq",
                &protos::frequency::basic::JoinFreqRequest { freq: freq.clone(), password: None },
            )?;
            if resp.status == protos::frequency::basic::FreqStatus::FreqIdle as i32 {
                crate::session::set_current(&freq);
            }
            print_content(&resp.message.clone().into());
        } else {
            print_content(&"[!] no background session; use `join_freq <freq>` first".into());
        }
        Ok(())
    }
}

impl CommandExecutor for ForegroundCommand {
    fn name(&self) -> &'static str {
        "foreground"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
