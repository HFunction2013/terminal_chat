// notifications.rs
// Poll and show pending notifications (message/call/mute/kick/ban ...)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;

pub struct NotificationsCommand;

impl NotificationsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        // 轮询场景：未连接 / 未设 USERNAME 时静默，不打扰 REPL。
        let resp: protos::frequency::notification::NotificationsResponse =
            match crate::connection::rpc(
                "notifications",
                &protos::frequency::notification::NotificationsRequest {},
            ) {
                Ok(r) => r,
                Err(_) => return Ok(()),
            };
        if resp.items.is_empty() {
            return Ok(());
        }
        // `setg QUIET on` 时静音全部通知。
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_global_option);
        if let TaggedOption::Some(v) = get_global_option(&"QUIET".into()) {
            let q = v.to_string().to_lowercase();
            if matches!(q.as_str(), "on" | "true" | "1" | "yes") {
                return Ok(());
            }
        }
        for it in &resp.items {
            print_content(&format!("!! [{}] {}", it.kind, it.text).into());
        }
        Ok(())
    }
}

impl CommandExecutor for NotificationsCommand {
    fn name(&self) -> &'static str {
        "notifications"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
