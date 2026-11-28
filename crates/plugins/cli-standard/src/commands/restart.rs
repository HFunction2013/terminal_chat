// restart.rs
// Reload runtime state; `--hard` restarts the client process
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;

pub struct RestartCommand;

impl RestartCommand {
    pub fn execute(&self, hard: bool) -> Result<()> {
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);

        if hard {
            // 硬重启：置位核心标志，由宿主主线程取走后以相同参数自拉起进程。
            cli_core_macros::load_core_symbols!(lib, request_hard_restart);
            let _ = request_hard_restart();
            print_content(&crate::tr("std.msg.restart.hard").into());
            return Ok(());
        }

        // 软重启：进程内回读磁盘持久化的别名 / 宏；插件挂载态与服务器连接保持不变。
        cli_core_macros::load_core_symbols!(lib, reload_aliases, reload_macros);
        let _ = reload_aliases();
        let _ = reload_macros();
        print_content(&crate::tr("std.msg.restart.soft").into());
        Ok(())
    }
}

impl CommandExecutor for RestartCommand {
    fn name(&self) -> &'static str {
        "restart"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let hard = matches.get_flag("hard");
        self.execute(hard)
    }
}
