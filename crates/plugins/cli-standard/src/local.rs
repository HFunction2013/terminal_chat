//! 本地状态管理：任务表、插件清单、workspace 目录、日志与代理开关。
//! 所有状态文件集中在 `~/.terminal_chat/` 下。
use std::fs;
use std::path::{Path, PathBuf};

pub const DIR_NAME: &str = ".terminal_chat";
pub const TASKS_FILE: &str = "tasks.txt";
pub const PLUGINS_DIR: &str = "plugins";
pub const INSTALLED_FILE: &str = "installed.txt";
pub const WORKSPACES_DIR: &str = "workspaces";
pub const CURRENT_WS_FILE: &str = "current_workspace";
pub const LOGGING_FILE: &str = "logging";
pub const PROXY_FILE: &str = "proxy";
pub const FEEDBACK_FILE: &str = "feedback.log";

/// 配置根目录 `~/.terminal_chat`（创建缺失目录）。
pub fn base_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join(DIR_NAME);
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn plugins_dir() -> PathBuf {
    let dir = base_dir().join(PLUGINS_DIR);
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn workspaces_dir() -> PathBuf {
    let dir = base_dir().join(WORKSPACES_DIR);
    let _ = fs::create_dir_all(&dir);
    dir
}

// ---------------- 任务表 ----------------
// 行格式: id \x1f type \x1f session \x1f at_secs \x1f payload_b64
// payload: cmd 任务为 base64(cmd)；msg 任务为 base64("message\x1fuser1,user2")
const SEP: char = '\x1f';

#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub id: u64,
    pub kind: String,    // "cmd" | "msg"
    pub session: String, // "0" = main console
    pub at: u64,         // unix 秒
    pub cmd: Option<String>,
    pub message: Option<String>,
    pub users: Vec<String>,
}

pub fn read_tasks() -> Vec<TaskEntry> {
    let path = base_dir().join(TASKS_FILE);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(SEP).collect();
        if parts.len() < 5 {
            continue;
        }
        let Ok(id) = parts[0].parse::<u64>() else { continue };
        let Ok(at) = parts[3].parse::<u64>() else { continue };
        let payload = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, parts[4]) {
            Ok(b) => String::from_utf8_lossy(&b).to_string(),
            Err(_) => continue,
        };
        let mut e = TaskEntry {
            id,
            kind: parts[1].to_string(),
            session: parts[2].to_string(),
            at,
            cmd: None,
            message: None,
            users: Vec::new(),
        };
        if e.kind == "cmd" {
            e.cmd = Some(payload);
        } else {
            let mut p = payload.split(SEP);
            e.message = p.next().map(|s| s.to_string());
            e.users = p.next().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
        }
        out.push(e);
    }
    out
}

pub fn write_tasks(tasks: &[TaskEntry]) -> std::io::Result<()> {
    let path = base_dir().join(TASKS_FILE);
    let mut content = String::new();
    for t in tasks {
        let payload = if t.kind == "cmd" {
            t.cmd.clone().unwrap_or_default()
        } else {
            format!("{}\x1f{}", t.message.clone().unwrap_or_default(), t.users.join(","))
        };
        let payload_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, payload.as_bytes());
        content.push_str(&format!("{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}\n",
            t.id, t.kind, t.session, t.at, payload_b64));
    }
    fs::write(path, content)
}

pub fn next_task_id(tasks: &[TaskEntry]) -> u64 {
    tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1
}

pub fn add_task(t: TaskEntry) -> std::io::Result<()> {
    let mut tasks = read_tasks();
    tasks.push(t);
    write_tasks(&tasks)
}

pub fn remove_task(id: u64) -> bool {
    let mut tasks = read_tasks();
    let before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() != before {
        let _ = write_tasks(&tasks);
        true
    } else {
        false
    }
}

/// 解析调度时间：支持 `HH:MM`（今天/明天）、`+Ns/+Nm/+Nh`（相对当前）、裸数字秒。
pub fn parse_time(spec: &str) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let spec = spec.trim();
    if let Ok(secs) = spec.parse::<u64>() {
        return Some(now + secs);
    }
    if let Some(rest) = spec.strip_prefix('+') {
        let (num, unit) = rest.split_at(rest.len().saturating_sub(1));
        if let Ok(n) = num.parse::<u64>() {
            let mult = match unit {
                "s" => 1,
                "m" => 60,
                "h" => 3600,
                _ => 1,
            };
            return Some(now + n * mult);
        }
    }
    // HH:MM
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() == 2 {
        if let (Ok(h), Ok(m)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
            if h <= 23 && m <= 59 {
                let mut secs_today = h * 3600 + m * 60;
                let day_start = now - now % 86400;
                if day_start + secs_today <= now {
                    secs_today += 86400;
                }
                return Some(day_start + secs_today);
            }
        }
    }
    None
}

// ---------------- 插件清单 ----------------
pub fn read_installed() -> Vec<String> {
    let path = base_dir().join(INSTALLED_FILE);
    match fs::read_to_string(&path) {
        Ok(c) => c.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn write_installed(names: &[String]) -> std::io::Result<()> {
    fs::write(base_dir().join(INSTALLED_FILE), names.join("\n"))
}

// ---------------- workspace ----------------
pub fn current_workspace() -> String {
    let path = base_dir().join(CURRENT_WS_FILE);
    fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

pub fn set_current_workspace(name: &str) -> std::io::Result<()> {
    fs::write(base_dir().join(CURRENT_WS_FILE), name)
}

pub fn workspace_path(name: &str) -> PathBuf {
    workspaces_dir().join(name)
}

pub fn list_workspaces() -> Vec<String> {
    match fs::read_dir(workspaces_dir()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = fs::read_dir(path) {
        for e in rd.flatten() {
            if let Ok(meta) = e.metadata() {
                if meta.is_dir() {
                    total += dir_size(&e.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

pub fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
