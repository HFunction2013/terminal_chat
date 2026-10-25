// msg.rs
// send message
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct MsgCommand;

impl MsgCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, message: Option<String>, users: Vec<String>) -> Result<()> {
        // 未提供消息文本时回退到系统编辑器。
        let message = match message {
            Some(m) if !m.trim().is_empty() => m,
            _ => crate::editor::open()?,
        };
        let resp: protos::messaging::message::SendMessageResponse =
            crate::connection::rpc("send_message", &protos::messaging::message::SendMessageRequest { message, users, freq: String::new() })?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for MsgCommand {
    fn name(&self) -> &'static str {
        "msg"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let message = matches.get_one::<String>("message").cloned();
        let users =
            matches.get_many::<String>("users").unwrap_or_default().cloned().collect::<Vec<_>>();
        self.execute(message, users)
    }
}
