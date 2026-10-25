//! 编辑器回退：命令未提供消息文本时打开 `$VISUAL` / `$EDITOR` 输入。
use anyhow::{Result, anyhow};
use std::io::Write;
use std::process::Command;

/// 打开系统编辑器让用户输入消息（缺省 `vi`）。
/// 以 `#` 开头的行视为注释被剥离；空内容返回错误。
pub fn open() -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let path = std::env::temp_dir().join(format!("tc_msg_{}.txt", std::process::id()));
    let _ = std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(b"# enter your message here\n"));
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"{}\"", editor, path.display()))
        .status()
        .map_err(|e| anyhow!("failed to launch editor `{editor}`: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(anyhow!("editor `{editor}` exited with {status}"));
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    let text = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.is_empty() {
        Err(anyhow!("empty message from editor; aborted"))
    } else {
        Ok(text)
    }
}
