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

/// 日志文件（`logs open` 后追加实际日志）。
pub const LOG_FILE: &str = "logging.log";

/// `logs open` 是否已开启（logging 开关文件内容为 "on"）。
pub fn logging_enabled() -> bool {
    fs::read_to_string(base_dir().join(LOGGING_FILE))
        .map(|s| s.trim() == "on")
        .unwrap_or(false)
}

/// 追加一行到日志文件（`logs open` 后生效；文件不存在则创建）。
pub fn append_log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state_dir().join(LOG_FILE))
    {
        let _ = writeln!(f, "{}", line);
    }
}

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
    let path = state_dir().join(TASKS_FILE);
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
    let path = state_dir().join(TASKS_FILE);
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

/// 解析调度时间，返回 unix 秒：
/// - 裸数字 / `+N`            → N 秒后（相对当前）
/// - `+Ns` / `+Nm` / `+Nh` / `+Nd` → 相对当前
/// - `HH:MM` / `HH:MM:SS`     → 本地时间的下一个该时刻（跨天自动顺延）
pub fn parse_time(spec: &str) -> Option<u64> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    // 相对时间：+N 带可选单位
    let relative: Option<(&str, i64)> = if let Some(rest) = spec.strip_prefix('+') {
        let last = rest.chars().last()?;
        match last {
            's' | 'm' | 'h' | 'd' => {
                let unit = match last {
                    's' => 1,
                    'm' => 60,
                    'h' => 3600,
                    _ => 86400,
                };
                Some((&rest[..rest.len() - 1], unit))
            }
            _ => Some((rest, 1)),
        }
    } else if spec.chars().all(|c| c.is_ascii_digit()) {
        Some((spec, 1))
    } else {
        None
    };
    if let Some((num, unit)) = relative {
        let n: i64 = num.parse().ok()?;
        return Some((chrono::Local::now() + chrono::Duration::seconds(n * unit)).timestamp() as u64);
    }
    // HH:MM[:SS] 按本地时区
    use chrono::Datelike;
    use chrono::TimeZone;
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() == 2 || parts.len() == 3 {
        let h: u32 = parts[0].parse().ok()?;
        let mi: u32 = parts[1].parse().ok()?;
        let s: u32 = if parts.len() == 3 { parts[2].parse().ok()? } else { 0 };
        if h > 23 || mi > 59 || s > 59 {
            return None;
        }
        let now = chrono::Local::now();
        let naive_today = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), now.day())?
            .and_hms_opt(h, mi, s)?;
        let mut target = naive_today;
        if target <= now.naive_local() {
            target += chrono::Duration::days(1);
        }
        return Some(chrono::Local.from_local_datetime(&target).single()?.timestamp() as u64);
    }
    None
}

/// 把 unix 秒格式化为本地 `MM-DD HH:MM:SS`。
pub fn format_local(at: u64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(at as i64, 0).single() {
        Some(t) => t.format("%m-%d %H:%M:%S").to_string(),
        None => at.to_string(),
    }
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

/// workspace 的独立 home 目录（切换 workspace 时把本地 cwd 切到这里）。
pub fn workspace_home(name: &str) -> PathBuf {
    workspace_path(name).join("home")
}

/// 当前 workspace 的状态根：`default` 沿用旧位置（~/.terminal_chat），
/// 其它 workspace 使用 `~/.terminal_chat/workspaces/<name>`，从而实现状态隔离
/// （任务表、日志开关、代理开关、feedback、别名/宏等各 workspace 独立）。
pub fn state_dir() -> PathBuf {
    let cur = current_workspace();
    if cur == "default" {
        base_dir()
    } else {
        let dir = workspaces_dir().join(&cur);
        let _ = fs::create_dir_all(&dir);
        dir
    }
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
