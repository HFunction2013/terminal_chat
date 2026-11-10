// sessions.rs
// Multi-session manager: list current + backgrounded freq sessions
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct SessionsCommand;

impl SessionsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, list: bool, interact: Option<String>, kill: Option<String>) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        // 一个 freq 是一个会话：当前会话在前台，其余在后台。
        // `sessions -i <freq>` 真正切换到目标会话；`sessions -k <freq>` 丢弃后台会话。
        if let Some(s) = interact {
            if crate::session::current().as_deref() == Some(s.as_str()) {
                print_content(&format!("[!] session `{s}` is already the current session").into());
                return Ok(());
            }
            if !crate::session::take_background(&s) {
                print_content(&format!("[!] no such session `{s}`; run `sessions -l` to list").into());
                return Ok(());
            }
            let resp: protos::frequency::basic::JoinFreqResponse = crate::connection::rpc(
                "join_freq",
                &protos::frequency::basic::JoinFreqRequest { freq: s.clone(), password: None },
            )?;
            if resp.status == protos::frequency::basic::FreqStatus::FreqIdle as i32 {
                crate::session::set_current(&s);
                print_content(&format!("[*] switched to session `{s}`").into());
            } else {
                // 恢复失败则放回后台列表，避免会话丢失。
                crate::session::background_current();
                print_content(&resp.message.clone().into());
            }
            return Ok(());
        }
        if let Some(s) = kill {
            if crate::session::current().as_deref() == Some(s.as_str()) {
                print_content(&format!("[!] cannot kill the current session `{s}`; use `leave_freq` or `background`").into());
                return Ok(());
            }
            if crate::session::take_background(&s) {
                print_content(&format!("[*] session `{s}` dropped").into());
            } else {
                print_content(&format!("[!] no such background session `{s}`").into());
            }
            return Ok(());
        }
        let _ = list;
        let (cur, bg) = crate::session::all();
        match &cur {
            Some(f) => print_content(&format!("  [0] session `{f}` (current)").into()),
            None => print_content(&"  [0] main console (no freq session)".into()),
        }
        for (i, f) in bg.iter().enumerate() {
            print_content(&format!("  [{}] session `{f}` (background)", i + 1).into());
        }
        Ok(())
    }
}

impl CommandExecutor for SessionsCommand {
    fn name(&self) -> &'static str {
        "sessions"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let list = matches.get_flag("list");
        let interact = matches.get_one::<String>("interact").cloned();
        let kill = matches.get_one::<String>("kill").cloned();
        self.execute(list, interact, kill)
    }
}
