// heartbeat.rs
// 长连接心跳命令：向服务端发 HeartbeatRequest（携带客户端时钟），
// 服务端返回 RTT + 待推送通知。REPL 每轮以心跳代替"轮询 notifications"。
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;

pub struct HeartbeatCommand;

impl HeartbeatCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        // 未连接 / 未设 USERNAME 时静默（由 srv-standard 对未知会话返回错误）。
        let client_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let resp: protos::connection::session::HeartbeatResponse =
            match crate::connection::rpc(
                "heartbeat",
                &protos::connection::session::HeartbeatRequest { client_ts },
            ) {
                Ok(r) => r,
                Err(_) => return Ok(()),
            };
        if resp.notifications.is_empty() {
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
        for it in &resp.notifications {
            print_content(&format!("!! [{}] {}", it.kind, it.text).into());
        }
        Ok(())
    }
}

impl CommandExecutor for HeartbeatCommand {
    fn name(&self) -> &'static str {
        "heartbeat"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
