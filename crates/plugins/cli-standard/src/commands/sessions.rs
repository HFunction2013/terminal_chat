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
        // 一个 freq 是一个会话：当前会话在前台，其余已在后台（可 `foreground` 恢复）。
        if let Some(s) = interact {
            print_content(&format!("[!] session `{s}` is in background; use `foreground` to switch back").into());
            return Ok(());
        }
        if let Some(s) = kill {
            print_content(&format!("[!] kill session `{s}`: use `foreground` then `leave_freq` to drop it").into());
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
