// shell.rs
// Run a local shell command (local machine, NOT remote)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct ShellCommand;

impl ShellCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, command: Vec<String>) -> Result<()> {
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if command.is_empty() {
            print_content(&"[!] usage: shell <command...>".into());
            return Ok(());
        }
        let cmdline = command.join(" ");
        // Windows 用 cmd /C，其它平台用 sh -c。
        let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
        let output = std::process::Command::new(shell)
            .arg(flag)
            .arg(&cmdline)
            .output()
            .map_err(|e| anyhow!("failed to spawn shell: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !stdout.is_empty() {
            print_content(&stdout.into());
        }
        if !stderr.is_empty() {
            print_content(&stderr.into());
        }
        print_content(&format!("[shell] exit code {}", output.status.code().unwrap_or(-1)).into());
        Ok(())
    }
}

impl CommandExecutor for ShellCommand {
    fn name(&self) -> &'static str {
        "shell"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let command: Vec<String> =
            matches.get_many::<String>("command").unwrap_or_default().cloned().collect();
        self.execute(command)
    }
}
