// freq_info.rs
// Get frequency information
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct FreqInfoCommand;

impl FreqInfoCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, frequency: String) -> Result<()> {
        // 参数为空时默认当前会话的 freq（仍为空则交由服务端按调用者所在 freq 解析）。
        let freq = if frequency.is_empty() {
            crate::session::current().unwrap_or_default()
        } else {
            frequency.clone()
        };
        let resp: protos::frequency::basic::FreqInfoResponse =
            crate::connection::rpc("freq_info", &protos::frequency::basic::FreqInfoRequest { freq })?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        match resp.info {
            Some(info) => {
                print_content(&format!("Freq:    {}", info.freq).into());
                print_content(&format!("Creator: {} ({})", info.creator_nickname, info.creator_ip).into());
                print_content(&format!("Members: {}/{}", info.current_members,
                    if info.max_members == 0 { "inf".to_string() } else { info.max_members.to_string() }).into());
                print_content(&format!("Password: {}", if info.has_password { "yes" } else { "no" }).into());
                for m in info.members {
                    print_content(&format!("  - {} ({})", m.nickname, m.ip_address).into());
                }
            }
            None => print_content(&format!("freq `{frequency}` not found").into()),
        }
        Ok(())
    }
}

impl CommandExecutor for FreqInfoCommand {
    fn name(&self) -> &'static str {
        "freq_info"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let frequency = matches
            .get_one::<String>("frequency")
            .ok_or_else(|| anyhow!("Missing required argument: frequency"))?
            .clone();
        self.execute(frequency)
    }
}
