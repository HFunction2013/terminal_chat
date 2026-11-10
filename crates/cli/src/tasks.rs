//! 本地调度任务到期检查（与 cli-standard 的 local.rs 共享同一任务文件格式）。
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use cli_core_types::RunCommandFn;
use std::path::PathBuf;

const SEP: char = '\x1f';

fn base_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".terminal_chat")
}

fn decode_b64(s: &str) -> Option<String> {
    let b = general_purpose::STANDARD.decode(s).ok()?;
    String::from_utf8(b).ok()
}

/// 检查到期任务并执行；执行过的任务从文件中移除。
pub fn check_and_run(run_command: &RunCommandFn) -> Result<()> {
    let path = base_dir().join("tasks.txt");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    if content.trim().is_empty() {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut remaining = String::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(SEP).collect();
        if parts.len() < 5 {
            continue;
        }
        let Ok(id) = parts[0].parse::<u64>() else {
            continue;
        };
        let Ok(at) = parts[3].parse::<u64>() else {
            continue;
        };
        if at > now {
            remaining.push_str(line);
            remaining.push('\n');
            continue;
        }
        let kind = parts[1];
        let payload = decode_b64(parts[4]).unwrap_or_default();
        // 构造 argv：cmd 任务为 shell_words 拆分；msg 任务直接拼 argv，避免引号/反斜杠被破坏。
        let args: Vec<String> = match kind {
            "cmd" => shell_words::split(&payload).unwrap_or_default(),
            "msg" => {
                let mut p = payload.split(SEP);
                let message = p.next().unwrap_or_default().to_string();
                let users = p.next().unwrap_or_default();
                let mut argv: Vec<String> = vec!["msg".to_string(), "-m".to_string(), message];
                argv.extend(users.split(',').filter(|u| !u.is_empty()).map(|u| u.to_string()));
                argv
            }
            _ => continue,
        };
        if args.is_empty() {
            continue;
        }
        let desc = args.join(" ");
        let args_ffi: safer_ffi::Vec<safer_ffi::String> =
            args.into_iter().map(|s| s.into()).collect::<Vec<_>>().into();
        let _ = run_command(&args_ffi);
        println!("[task #{id}] executed: {desc}");
    }
    std::fs::write(&path, remaining)?;
    Ok(())
}
