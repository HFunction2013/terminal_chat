// editor.rs
// just opens an editor
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::{Context, Result};
use clap::ArgMatches;
use cli_core::global_settings::get_global_option;
use std::path::PathBuf;
use std::process::Command;
use which::which;

fn find_editor(name: &str) -> Option<PathBuf> {
    if let Ok(path) = which(name) {
        return Some(path);
    }
    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join(name);
        if local.is_file() {
            return Some(local);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join(name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }

    None
}

pub struct EditorCommand;

impl EditorCommand {
    const CANDIDATES: &[&str] = {
        #[cfg(target_os = "windows")]
        {
            &["vim", "hx", "nvim", "nano", "micro", "emacs", "notepad++", "notepad", "code"]
        }

        #[cfg(target_os = "macos")]
        {
            &["vim", "hx", "nvim", "nano", "micro", "emacs", "code", "subl", "zed"]
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            &["micro", "vim", "hx", "nvim", "nano", "emacs", "code", "subl", "zed", "kak", "gedit"]
        }
    };

    fn choose_editor() -> Option<String> {
        for editor in Self::CANDIDATES {
            if let Some(path) = find_editor(editor) {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }
}

impl CommandExecutor for EditorCommand {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let file = matches.get_one::<String>("file");

        let editor = matches
            .get_one::<String>("editor")
            .filter(|s| !s.is_empty())
            .cloned()
            .or_else(|| get_global_option("EDITOR"))
            .or_else(Self::choose_editor);

        let editor_path = match editor {
            Some(ref name) => {
                find_editor(name).with_context(|| format!("cannot find editor: {name}"))?
            }
            None => anyhow::bail!(
                "no editor configured.\n\
                 Use --editor <name>, or set it globally."
            ),
        };

        let mut cmd = Command::new(&editor_path);
        if let Some(f) = file {
            cmd.arg(f);
        }

        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        let status = cmd
            .status()
            .with_context(|| format!("failed to start editor: {}", editor_path.display()))?;

        if !status.success() {
            anyhow::bail!("editor exited with status: {status}");
        }

        Ok(())
    }
}
