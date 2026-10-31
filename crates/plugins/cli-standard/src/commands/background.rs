// background.rs
// Background current session (the current freq) and return to main console
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct BackgroundCommand;

impl BackgroundCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        // 一个 freq 是一个会话：background 挂起当前 freq（服务端 leave_freq，
        // 本地放入后台列表），之后可随时 `foreground` 恢复。
        if let Some(freq) = crate::session::background_current() {
            let _resp: protos::frequency::basic::LeaveFreqResponse = crate::connection::rpc(
                "leave_freq",
                &protos::utils::empty::Empty {},
            )?;
            print_content(&format!("[!] session `{freq}` backgrounded (left freq)").into());
        } else {
            print_content(&"[!] no active session to background".into());
        }
        Ok(())
    }
}

impl CommandExecutor for BackgroundCommand {
    fn name(&self) -> &'static str {
        "background"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
