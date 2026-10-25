// reply.rs
// Reply to the user who has just spoken quickly
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct ReplyCommand;

impl ReplyCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, message: Option<String>) -> Result<()> {
        // 未提供消息文本时回退到系统编辑器。
        let message = match message {
            Some(m) if !m.trim().is_empty() => m,
            _ => crate::editor::open()?,
        };
        let resp: protos::messaging::message::ReplyMessageResponse =
            crate::connection::rpc("reply_message", &protos::messaging::message::ReplyMessageRequest { message })?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for ReplyCommand {
    fn name(&self) -> &'static str {
        "reply"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let message = matches.get_one::<String>("message").cloned();
        self.execute(message)
    }
}
