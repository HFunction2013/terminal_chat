// shell.rs
// Run a local shell command (local machine, NOT remote)
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::process::Stdio;

pub struct ShellCommand;

impl ShellCommand {
    pub fn execute(&self, command: Vec<String>) -> Result<()> {
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if command.is_empty() {
            print_content(&"[!] usage: shell <command...>".into());
            return Ok(());
        }
        let cmdline = command.join(" ");
        // Windows 用 cmd /C，其它平台用 sh -c。子进程继承终端，可运行交互式
        // TUI 程序（vim/top 等）；拉起前后由终端钩子离开 / 重进全屏（见 #70）。
        let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
        let mut cmd = std::process::Command::new(shell);
        cmd.arg(flag)
            .arg(&cmdline)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        crate::terminal::suspend();
        let status = cmd.status().map_err(|e| anyhow!("failed to spawn shell: {e}"));
        crate::terminal::resume();

        let status = status?;
        let code = status.code().unwrap_or(-1);
        print_content(&format!("[shell] exit code {code}").into());
        Ok(())
    }
}

impl CommandExecutor for ShellCommand {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let command: Vec<String> =
            matches.get_many::<String>("command").unwrap_or_default().cloned().collect();
        self.execute(command)
    }
}
