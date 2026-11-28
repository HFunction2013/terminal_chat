// shell.rs
// Run a local shell command (local machine, NOT remote).
// Default: capture stdout/stderr and echo to the TUI output area; stdin is not inherited.
// `--tui`: inherit the terminal to run an interactive TUI program.
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::process::Stdio;
use std::time::Instant;

pub struct ShellCommand;

impl ShellCommand {
    pub fn execute(&self, command: Vec<String>, tui: bool) -> Result<()> {
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if command.is_empty() {
            print_content(&crate::tr("std.err.misc.usage_shell").into());
            return Ok(());
        }
        let cmdline = command.join(" ");
        let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
        // 子进程沿用本地工作目录（lpwd 报告 / lcd 切换的进程 cwd）。
        let cwd = std::env::current_dir()?;

        let started = Instant::now();
        let io_result: std::io::Result<(std::process::ExitStatus, Option<std::process::Output>)> =
            if tui {
                let mut cmd = std::process::Command::new(shell);
                cmd.arg(flag)
                    .arg(&cmdline)
                    .current_dir(&cwd)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());
                // 交互式 TUI：拉起前离开全屏，退出后重进（见 #70）。
                crate::terminal::suspend();
                let status = cmd.status();
                crate::terminal::resume();
                status.map(|s| (s, None))
            } else {
                let mut cmd = std::process::Command::new(shell);
                cmd.arg(flag)
                    .arg(&cmdline)
                    .current_dir(&cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                cmd.output().map(|o| (o.status, Some(o)))
            };
        let (status, output) = io_result.map_err(|e| anyhow!("failed to spawn shell: {e}"))?;

        // 回显捕获到的 stdout/stderr（finish 行之前）。
        if let Some(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
            if !stdout.is_empty() {
                print_content(&stdout.into());
            }
            if !stderr.is_empty() {
                print_content(&stderr.into());
            }
        }

        let elapsed = started.elapsed().as_secs_f64();
        let code = status.code().unwrap_or(-1);
        print_content(
            &crate::tr("std.prompt.misc.shell_finish")
                .replace("{secs}", &format!("{elapsed:.3}"))
                .replace("{code}", &code.to_string())
                .into(),
        );
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
        let tui = matches.get_flag("tui");
        self.execute(command, tui)
    }
}
