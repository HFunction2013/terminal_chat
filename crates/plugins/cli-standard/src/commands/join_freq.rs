// join_freq.rs
// Join a target frequency channel
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct JoinFreqCommand;

impl JoinFreqCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, freq: String) -> Result<()> {
        let resp: protos::frequency::basic::JoinFreqResponse =
            crate::connection::rpc("join_freq", &protos::frequency::basic::JoinFreqRequest { freq: freq.clone(), password: None })?;
        // 加入成功（FreqIdle）即成为当前会话：一个 freq 是一个会话。
        if resp.status == protos::frequency::basic::FreqStatus::FreqIdle as i32 {
            crate::session::set_current(&freq);
        }
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for JoinFreqCommand {
    fn name(&self) -> &'static str {
        "join_freq"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let freq = matches
            .get_one::<String>("freq")
            .ok_or_else(|| anyhow!("Missing required argument: freq"))?
            .clone();
        self.execute(freq)
    }
}
