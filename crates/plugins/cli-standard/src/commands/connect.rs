// connect.rs
// Connect the server
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct ConnectCommand;

/// 写一条全局选项（参照 `setg` 命令通路）。
fn set_global(key: &str, value: &str) -> Result<()> {
    let lib = LIB.get().expect("`cli-core` not initialized");
    cli_core_macros::load_core_symbols!(lib, set_global_option);
    let option = GlobalOption::new(key, value);
    let mut echo: ManuallyDrop<GlobalOption> = ManuallyDrop::new(GlobalOption::default());
    let _res = set_global_option(&option, Out::from(&mut echo));
    Ok(())
}

impl ConnectCommand {
    /// `server` 可选：`HOST[:PORT]`；传入时拆分回写 SERVER / PORT 后再连接。
    pub fn execute(&self, server: Option<String>) -> Result<()> {
        if let Some(addr) = server {
            let addr = addr.trim().to_string();
            if !addr.is_empty() {
                let (host, port) = crate::connection::split_host_port(&addr);
                if !host.is_empty() {
                    set_global("SERVER", &host)?;
                }
                if let Some(p) = port {
                    set_global("PORT", &p)?;
                }
            }
        }
        crate::connection::connect_now()
    }
}

impl CommandExecutor for ConnectCommand {
    fn name(&self) -> &'static str {
        "connect"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let server = matches.get_one::<String>("server").cloned();
        self.execute(server)
    }
}
