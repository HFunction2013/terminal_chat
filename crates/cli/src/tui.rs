//! `--tui` 模式：基于 vendored microsoft/edit TUI 框架（`edit-tui`）的全屏聊天软件界面。
//!
//! 布局（类 GUI 聊天客户端）：
//! ```text
//! ┌ Connection  Workspace  Frequency  Message  ... Help ┐  ← 顶部菜单栏（下拉）
//! ├──────────────┬──────────────────────────────────────┤
//! │ 会话 / 成员   │  聊天记录区（推送 + 命令执行记录）         │  ← 分栏主体
//! ├──────────────┴──────────────────────────────────────┤
//! │ > 正在输入...                                        │  ← 输入框
//! ├──────────────────────────────────────────────────────┤
//! │ user@server:port | freq | QUIET | ...               │  ← 状态栏
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! 框架与 retained-mode 组件树源自 microsoft/edit（MIT，Copyright (c) Microsoft
//! Corporation），已 vendored 到本仓库 `crates/edit-tui`；本模块为聊天客户端界面层。
//!
//! 交互：菜单可用鼠标点击，或 `F10`/`Alt+助记符` 展开；输入框 `Enter` 执行；
//! `Ctrl+Q` 退出。

use anyhow::Result;
use cli_core_types::RunCommandFn;
use safer_ffi::option::TaggedOption;
use std::time::Duration;

use edit_tui::arena::{self, scratch_arena};
use edit_tui::framebuffer::IndexedColor;
use edit_tui::helpers::{COORD_TYPE_SAFE_MAX, CoordType, MEBI, Point, Rect, Size};
use edit_tui::input::{self, Input, kbmod, vk};
use edit_tui::sys;
use edit_tui::tui::{Context, Tui};
use edit_tui::vt;

/// 拉取积压推送的 FFI 符号类型。
pub type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

/// 读取全局选项的 FFI 符号类型（cli-core `get_global_option`）。
pub type GetOptionFn = unsafe extern "C" fn(&safer_ffi::String) -> TaggedOption<safer_ffi::String>;

/// 切换 TUI/REPL 输出模式的 FFI 符号类型。
pub type SetTuiModeFn = unsafe extern "C" fn(bool);

/// 取走 TUI 命令输出缓冲的 FFI 符号类型。
pub type OutputDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

#[cfg(target_pointer_width = "32")]
const SCRATCH_ARENA_CAPACITY: usize = 128 * MEBI;
#[cfg(target_pointer_width = "64")]
const SCRATCH_ARENA_CAPACITY: usize = 512 * MEBI;

/// 聊天记录行的着色类别。
#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    /// 命令回显（用户输入）。
    Cmd,
    /// 服务端推送。
    Push,
    /// 系统提示 / 本地状态。
    System,
    /// 命令结果或错误。
    Result,
}

impl LineKind {
    fn color(self) -> IndexedColor {
        match self {
            LineKind::Cmd => IndexedColor::Cyan,
            LineKind::Push => IndexedColor::Yellow,
            LineKind::System => IndexedColor::BrightBlack,
            LineKind::Result => IndexedColor::Green,
        }
    }
}

struct ChatLine {
    text: String,
    kind: LineKind,
}

/// 菜单项动作。
enum Action {
    /// 直接执行 argv。
    Run(Vec<&'static str>),
    /// 把命令前缀预填进输入框，交用户补全参数。
    Prefill(&'static str),
}

struct MenuItem {
    label: &'static str,
    action: Action,
}

struct MenuGroup {
    name: &'static str,
    accel: char,
    items: Vec<MenuItem>,
}

/// 输入框命令名补全候选（与 cli-standard 注册命令一致的静态子集）。
const COMMAND_NAMES: &[&str] = &[
    "away",
    "background",
    "ban",
    "call",
    "cancel",
    "cd",
    "checksum",
    "connect",
    "cp",
    "create",
    "create_freq",
    "deop",
    "delete",
    "disband",
    "disconnect",
    "download",
    "editor",
    "exit",
    "feedback",
    "file_info",
    "foreground",
    "freq_info",
    "get",
    "getg",
    "help",
    "ignore",
    "join_freq",
    "kick",
    "lcd",
    "leave_freq",
    "lls",
    "load",
    "lpwd",
    "ls",
    "mkdir",
    "msg",
    "mute",
    "mv",
    "notifications",
    "op",
    "proxy",
    "pwd",
    "quit",
    "reconnect",
    "reply",
    "rm",
    "route",
    "run",
    "schedule",
    "schedule_msg",
    "session",
    "sessions",
    "set",
    "setg",
    "shell",
    "show",
    "sleep",
    "status",
    "tasks",
    "tree",
    "trop",
    "unban",
    "unignore",
    "unload",
    "unmute",
    "unset",
    "unsetg",
    "upload",
    "user_info",
    "whoami",
    "workspace",
];

/// 菜单定义：按 GUI 软件的大功能分组。
fn menu_groups() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            name: "Connection",
            accel: 'C',
            items: vec![
                MenuItem { label: "Connect", action: Action::Run(vec!["connect"]) },
                MenuItem { label: "Reconnect", action: Action::Run(vec!["reconnect"]) },
                MenuItem { label: "Disconnect", action: Action::Run(vec!["disconnect"]) },
                MenuItem { label: "Exit", action: Action::Run(vec!["exit"]) },
            ],
        },
        MenuGroup {
            name: "Workspace",
            accel: 'W',
            items: vec![
                MenuItem { label: "List", action: Action::Run(vec!["workspace", "list"]) },
                MenuItem { label: "Switch…", action: Action::Prefill("workspace switch ") },
                MenuItem { label: "New…", action: Action::Prefill("workspace new ") },
                MenuItem { label: "Show", action: Action::Run(vec!["workspace", "show"]) },
            ],
        },
        MenuGroup {
            name: "Frequency",
            accel: 'F',
            items: vec![
                MenuItem { label: "Join…", action: Action::Prefill("join_freq ") },
                MenuItem { label: "Create…", action: Action::Prefill("create_freq ") },
                MenuItem { label: "Leave", action: Action::Run(vec!["leave_freq"]) },
                MenuItem { label: "Info", action: Action::Run(vec!["freq_info"]) },
                MenuItem { label: "Background", action: Action::Run(vec!["background"]) },
                MenuItem { label: "Sessions", action: Action::Run(vec!["sessions", "list"]) },
            ],
        },
        MenuGroup {
            name: "Message",
            accel: 'M',
            items: vec![
                MenuItem { label: "Send…", action: Action::Prefill("msg -m ") },
                MenuItem { label: "Reply…", action: Action::Prefill("reply ") },
                MenuItem { label: "Call…", action: Action::Prefill("call ") },
            ],
        },
        MenuGroup {
            name: "Moderation",
            accel: 'O',
            items: vec![
                MenuItem { label: "Mute…", action: Action::Prefill("mute ") },
                MenuItem { label: "Unmute…", action: Action::Prefill("unmute ") },
                MenuItem { label: "Ignore…", action: Action::Prefill("ignore ") },
                MenuItem { label: "Kick…", action: Action::Prefill("kick ") },
                MenuItem { label: "Ban…", action: Action::Prefill("ban ") },
                MenuItem { label: "Disband…", action: Action::Prefill("disband ") },
            ],
        },
        MenuGroup {
            name: "Files",
            accel: 'L',
            items: vec![
                MenuItem { label: "List…", action: Action::Prefill("ls ") },
                MenuItem { label: "Upload…", action: Action::Prefill("upload ") },
                MenuItem { label: "Download…", action: Action::Prefill("download ") },
                MenuItem { label: "Tree", action: Action::Run(vec!["tree"]) },
                MenuItem { label: "Remove…", action: Action::Prefill("rm ") },
                MenuItem { label: "Move…", action: Action::Prefill("mv ") },
                MenuItem { label: "Copy…", action: Action::Prefill("cp ") },
            ],
        },
        MenuGroup {
            name: "Plugins",
            accel: 'P',
            items: vec![
                MenuItem { label: "Show", action: Action::Run(vec!["show", "plugins"]) },
                MenuItem { label: "Load…", action: Action::Prefill("load ") },
                MenuItem { label: "Unload…", action: Action::Prefill("unload ") },
            ],
        },
        MenuGroup {
            name: "Help",
            accel: 'H',
            items: vec![
                MenuItem { label: "Options", action: Action::Run(vec!["show", "options"]) },
                MenuItem { label: "Locals", action: Action::Run(vec!["show", "locals"]) },
                MenuItem {
                    label: "Active freqs",
                    action: Action::Run(vec!["show", "active_freqs"]),
                },
                MenuItem {
                    label: "Active users",
                    action: Action::Run(vec!["show", "active_users"]),
                },
                MenuItem { label: "Shell…", action: Action::Prefill("shell ") },
            ],
        },
    ]
}

/// `connect` 缺配置时的弹窗询问阶段。
#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogStage {
    /// 先问服务器地址（SERVER[:PORT]）。
    AskServer,
    /// 再问昵称（USERNAME）。
    AskUser,
}

/// 弹窗收集到的连接配置；两阶段都确认后回填全局选项并执行 `connect`。
struct Dialog {
    stage: DialogStage,
    server: String,
    username: String,
}

struct App {
    /// 聊天记录。
    log: Vec<ChatLine>,
    /// 输入缓冲（作为 editline 的 `WriteableDocument`）。
    input: String,
    /// 命令历史。
    history: Vec<String>,
    /// 历史翻页位置：`None` 表示正在编辑新命令。
    history_idx: Option<usize>,
    /// 是否跟随最新消息（`false` 表示用户向上回看，新消息不强制滚到底）。
    follow: bool,
    /// 聊天记录向上回看的行数（`0` 表示贴底）。
    scroll_y: usize,
    /// 当前输入前缀匹配出的补全候选。
    completions: Vec<String>,
    /// 候选循环下标。
    comp_idx: usize,
    /// 激活中的配置弹窗；`None` 表示无弹窗。
    dialog: Option<Dialog>,
    /// 侧栏快照：当前会话 freq。
    current_freq: Option<String>,
    /// 侧栏快照：收集到的成员名。
    members: Vec<String>,
    /// 状态栏快照（来自全局选项）。
    username: String,
    server: String,
    port: String,
    quiet: bool,
    /// 退出标志。
    exit: bool,
}

impl App {
    fn new() -> Self {
        let mut log = Vec::new();
        log.push(ChatLine {
            text: "[tui] 聊天界面：F10/Alt 展开菜单，Enter 执行命令，Tab 补全，Ctrl+Q 退出。"
                .to_string(),
            kind: LineKind::System,
        });
        Self {
            log,
            input: String::new(),
            history: Vec::new(),
            history_idx: None,
            follow: true,
            scroll_y: 0,
            completions: Vec::new(),
            comp_idx: 0,
            dialog: None,
            current_freq: None,
            members: Vec::new(),
            username: String::new(),
            server: String::new(),
            port: String::new(),
            quiet: false,
            exit: false,
        }
    }

    fn push_line(&mut self, text: String, kind: LineKind) {
        for line in text.lines() {
            self.log.push(ChatLine { text: line.to_string(), kind });
        }
    }

    /// 从一条推送文本中收集成员名，并更新当前会话快照。
    fn note_push(&mut self, display: &str) {
        // display 形如 `!! [join:0.5] alice joined the freq`
        let kind = display.split('[').nth(1).and_then(|s| s.split(']').next()).unwrap_or("");
        let rest = display.split_once(']').map(|(_, r)| r.trim()).unwrap_or(display);
        let kind_name = kind.split(':').next().unwrap_or(kind);

        match kind_name {
            "join" => {
                if let Some(name) = rest.split_whitespace().next() {
                    self.add_member(name);
                }
            }
            "leave" => {
                if let Some(name) = rest.split_whitespace().next() {
                    self.members.retain(|m| m != name);
                }
            }
            "msg" => {
                if let Some(name) = rest.split(':').next() {
                    self.add_member(name.trim());
                }
            }
            _ => {}
        }
    }

    fn add_member(&mut self, name: &str) {
        if !name.is_empty() && !self.members.iter().any(|m| m == name) {
            self.members.push(name.to_string());
        }
    }

    /// 用 `freq_info` / `show active_users` 等命令的成员列表回填侧栏。
    ///
    /// 支持的行格式：
    /// - `freq_info`：`  - <nick> (<ip>)`、`Creator: <nick> (<ip>)`
    /// - `active_users`：`  <user> <nickname> [<flags>]`（取昵称列，昵称空时回退账号列）
    fn reconcile_members(&mut self, text: &str) {
        let mut names: Vec<String> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }

            // freq_info 格式。
            if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("Creator: ")) {
                let name = rest.split(" (").next().unwrap_or(rest).trim().to_string();
                push_unique(&mut names, &name);
                continue;
            }

            // active_users 格式；跳过汇总行（如 `3 user(s) in freq \`xxx\``）。
            if looks_like_summary_line(t) {
                continue;
            }
            let tokens: Vec<&str> = t.split_whitespace().collect();
            if let Some(name) = tokens.get(1).or(tokens.first()) {
                push_unique(&mut names, name);
            }
        }
        if !names.is_empty() {
            self.members = names;
        }
    }

    /// 历史上翻：取更早一条命令。
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_idx {
            Some(i) => i.saturating_sub(1),
            None => self.history.len() - 1,
        };
        self.history_idx = Some(idx);
        self.input = self.history[idx].clone();
    }

    /// 历史下翻：取更晚一条，回到末尾时清空输入框。
    fn history_next(&mut self) {
        match self.history_idx {
            Some(i) if i + 1 < self.history.len() => {
                let idx = i + 1;
                self.history_idx = Some(idx);
                self.input = self.history[idx].clone();
            }
            _ => {
                self.history_idx = None;
                self.input.clear();
            }
        }
    }

    /// 向上回看一页：脱离跟随，滚动偏移增加一页。
    fn scroll_up(&mut self, page: usize) {
        self.follow = false;
        self.scroll_y = self.scroll_y.saturating_add(page.max(1));
    }

    /// 向下翻一页；回到底部时恢复跟随最新消息。
    fn scroll_down(&mut self, page: usize) {
        let page = page.max(1);
        if self.scroll_y <= page {
            self.scroll_y = 0;
            self.follow = true;
        } else {
            self.scroll_y -= page;
        }
    }

    /// 打开连接配置弹窗：先问缺失的服务器地址，再问缺失的昵称。
    fn open_connect_dialog(&mut self) {
        let stage =
            if self.server.is_empty() { DialogStage::AskServer } else { DialogStage::AskUser };
        self.dialog =
            Some(Dialog { stage, server: self.server.clone(), username: self.username.clone() });
    }

    /// 返回弹窗当前阶段正在编辑的文本缓冲。
    fn dialog_buffer(&mut self) -> Option<&mut String> {
        let d = self.dialog.as_mut()?;
        Some(match d.stage {
            DialogStage::AskServer => &mut d.server,
            DialogStage::AskUser => &mut d.username,
        })
    }

    /// 回到最新消息并恢复跟随。
    fn follow_latest(&mut self) {
        self.follow = true;
        self.scroll_y = 0;
    }

    /// 根据当前输入刷新补全候选：首词匹配命令名，后续词匹配成员名。
    fn refresh_completions(&mut self) {
        let parts: Vec<&str> = self.input.split_whitespace().collect();
        let cands: Vec<String> = match parts.as_slice() {
            [] => Vec::new(),
            [prefix] => COMMAND_NAMES
                .iter()
                .copied()
                .filter(|c| c.starts_with(*prefix))
                .map(String::from)
                .collect(),
            _ => {
                let prefix = parts[parts.len() - 1];
                self.members.iter().filter(|m| m.starts_with(prefix)).cloned().collect()
            }
        };
        self.completions = cands;
        if self.comp_idx >= self.completions.len() {
            self.comp_idx = 0;
        }
    }

    /// 接受当前高亮候选：首词替换为命令名，其余位置替换为成员名。
    fn accept_completion(&mut self) {
        let Some(cand) = self.completions.get(self.comp_idx).cloned() else {
            return;
        };
        let mut parts: Vec<&str> = self.input.split_whitespace().collect();
        if parts.len() <= 1 {
            self.input = cand;
        } else {
            parts.pop();
            parts.push(cand.as_str());
            self.input = parts.join(" ");
        }
        self.refresh_completions();
    }

    /// 根据刚执行的命令更新本地会话快照。
    fn note_command(&mut self, argv: &[String]) {
        let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
        match cmd {
            "join_freq" | "create_freq" => {
                if let Some(f) = argv.get(1) {
                    self.current_freq = Some(f.clone());
                }
            }
            "leave_freq" | "disband" => self.current_freq = None,
            _ => {}
        }
    }
}

/// 计算候选相对当前输入末词的剩余补全片段（灰色 ghost 文本）。
fn ghost_suffix(input: &str, cand: &str) -> String {
    let prefix = input.split_whitespace().last().unwrap_or(input);
    cand.strip_prefix(prefix).unwrap_or_default().to_string()
}

/// 向成员列表追加一个去重后的非空名字。
fn push_unique(list: &mut Vec<String>, name: &str) {
    let n = name.trim().to_string();
    if !n.is_empty() && !list.iter().any(|x| x == &n) {
        list.push(n);
    }
}

/// 判断一行是否为命令汇总/提示文本，而非成员条目（避免把计数行回填进侧栏）。
fn looks_like_summary_line(t: &str) -> bool {
    let lower = t.to_lowercase();
    lower.contains("user(s)")
        || lower.contains("in freq")
        || lower.contains("not found")
        || lower.contains("not in any freq")
        || lower.starts_with("freq:")
        || lower.starts_with("members:")
        || lower.starts_with("password:")
        || t.split_whitespace()
            .next()
            .map(|first| first.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn opt_get(get: &GetOptionFn, key: &str) -> String {
    match unsafe { get(&key.into()) } {
        TaggedOption::Some(v) => v.to_string(),
        TaggedOption::None => String::new(),
    }
}

fn is_quiet(v: &str) -> bool {
    matches!(v.to_lowercase().as_str(), "on" | "true" | "1" | "yes")
}

/// 终端模式恢复：退出时写回 alt screen / 鼠标 / 粘贴模式。
struct RestoreModes;

impl Drop for RestoreModes {
    fn drop(&mut self) {
        sys::write_stdout("\x1b[0 q\x1b[?25h\x1b]0;\x07\x1b[?1002;1006;2004l\x1b[?1049l");
    }
}

/// 运行 TUI 主循环（edit retained-mode 骨架）。
pub fn run(
    run_command: &RunCommandFn,
    take_pushes: &PushDrainFn,
    get_option: &GetOptionFn,
    set_tui_mode: &SetTuiModeFn,
    output_drain: &OutputDrainFn,
) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("--tui requires an interactive terminal (stdout is not a tty)");
    }

    // 进入 TUI 模式：命令输出改走内存缓冲，不再直写终端。
    unsafe { set_tui_mode(true) };

    let _sys_deinit = sys::init()?;
    arena::init(SCRATCH_ARENA_CAPACITY)?;

    let mut vt_parser = vt::Parser::new();
    let mut input_parser = input::Parser::new();
    let mut tui = Tui::new()?;

    let initial_size = sys::switch_modes()?;
    tui.set_size(initial_size);

    // 进 alternate screen、鼠标跟踪、括号粘贴（跳过 OSC 颜色探测，用默认主题）。
    sys::write_stdout("\x1b[?1049h\x1b[?1002;1006;2004h\x1b[?1036h");
    let _restore = RestoreModes;

    let mut app = App::new();
    let res = event_loop(
        &mut tui,
        &mut app,
        &mut vt_parser,
        &mut input_parser,
        run_command,
        take_pushes,
        get_option,
        output_drain,
    );

    res
}

fn event_loop(
    tui: &mut Tui,
    app: &mut App,
    vt_parser: &mut vt::Parser,
    input_parser: &mut input::Parser,
    run_command: &RunCommandFn,
    take_pushes: &PushDrainFn,
    get_option: &GetOptionFn,
    output_drain: &OutputDrainFn,
) -> Result<()> {
    let groups = menu_groups();

    loop {
        // 刷新状态栏快照（全局选项）。
        app.username = opt_get(get_option, "USERNAME");
        app.server = opt_get(get_option, "SERVER");
        app.port = opt_get(get_option, "PORT");
        app.quiet = is_quiet(&opt_get(get_option, "QUIET"));

        // 取积压推送。
        let pushes: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
        for p in pushes {
            let text = p.to_string();
            app.note_push(&text);
            app.push_line(text, LineKind::Push);
        }

        // 读取一批输入（超时较短，便于定时拉取推送）。
        {
            let scratch = scratch_arena(None);
            let read_timeout =
                vt_parser.read_timeout().min(tui.read_timeout()).min(Duration::from_millis(100));
            let read = sys::read_stdin(&scratch, read_timeout);

            if let Some((resize, input)) = read {
                if let Some(size) = resize {
                    draw(tui, Some(Input::Resize(size)), app, run_command, output_drain, &groups);
                }

                let vt_iter = vt_parser.parse(&input);
                let mut input_iter = input_parser.parse(vt_iter);
                while let Some(input) = input_iter.next() {
                    draw(tui, Some(input), app, run_command, output_drain, &groups);
                }
            }
        }

        // 布局稳定前持续重绘。
        while tui.needs_settling() {
            draw(tui, None, app, run_command, output_drain, &groups);
        }

        if app.exit {
            break;
        }

        let scratch = scratch_arena(None);
        let output = tui.render(&scratch);
        sys::write_stdout(&output);
    }

    Ok(())
}

fn draw(
    tui: &mut Tui,
    input: Option<Input>,
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    groups: &[MenuGroup],
) {
    let ctx = &mut tui.create_context(input);

    draw_menubar(ctx, app, run_command, output_drain, groups);
    draw_body(ctx, app);
    draw_input(ctx, app, run_command, output_drain);
    draw_status(ctx, app);
    draw_dialog(ctx, app, run_command, output_drain);

    if let Some(key) = ctx.keyboard_input() {
        if key == kbmod::CTRL | vk::Q {
            app.exit = true;
            ctx.set_input_consumed();
        } else if app.dialog.is_none() && key == vk::PRIOR {
            // PgUp：聊天记录向上回看一页。
            let page = (ctx.size().height - 3).max(1).max(0) as usize;
            app.scroll_up(page);
            ctx.set_input_consumed();
        } else if app.dialog.is_none() && key == vk::NEXT {
            // PgDn：聊天记录向下翻一页，回到底部恢复跟随。
            let page = (ctx.size().height - 3).max(1).max(0) as usize;
            app.scroll_down(page);
            ctx.set_input_consumed();
        }
    }
}

fn draw_menubar(
    ctx: &mut Context,
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    groups: &[MenuGroup],
) {
    ctx.menubar_begin();
    // 菜单栏撑满整行宽度，与下方主体 / 输入框 / 状态栏左右边缘对齐。
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    for group in groups {
        if ctx.menubar_menu_begin(group.name, group.accel) {
            for item in &group.items {
                let clicked = ctx.menubar_menu_button(item.label, '\0', vk::NULL);
                if clicked {
                    run_action(app, run_command, output_drain, &item.action);
                }
            }
            ctx.menubar_menu_end();
        }
    }
    ctx.menubar_end();
}

fn draw_body(ctx: &mut Context, app: &App) {
    // 顶层纵向：菜单栏(1) + 主体 + 输入框(1) + 状态栏(1)。主体高度显式减去其余三行。
    let body_height = (ctx.size().height - 3).max(1);

    ctx.table_begin("body");
    ctx.table_set_columns(&[26, COORD_TYPE_SAFE_MAX]);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
    ctx.table_next_row();

    // 左侧：会话 / 成员。
    ctx.block_begin("sidebar");
    ctx.attr_padding(Rect::two(0, 1));
    ctx.attr_border();
    ctx.attr_intrinsic_size(Size { width: 26, height: body_height });

    ctx.label("sec-freq", "会话");
    let freq = app.current_freq.clone().unwrap_or_else(|| "(未加入)".to_string());
    ctx.label("freq", &freq);
    ctx.label("blank", "");

    ctx.label("sec-members", "成员");
    if app.members.is_empty() {
        ctx.label("none", "  (暂无)");
    } else {
        for (i, m) in app.members.iter().enumerate() {
            ctx.next_block_id_mixin(i as u64);
            ctx.label("member", &format!("  • {m}"));
        }
    }
    ctx.block_end();

    // 右侧：聊天记录滚动区。全量喂入，由框架 scrollarea 处理滚轮 / 滚动条 /
    // 翻页；`follow` 时贴底，否则按回看行数上移。
    let total = app.log.len();
    let view = body_height.max(0) as usize;
    let base = total.saturating_sub(view);
    let off = if app.follow { base } else { base + app.scroll_y };

    ctx.scrollarea_begin("chat", Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
    // 必须在加入子节点之前定位滚动偏移：begin 返回时 last_node 仍是挂
    // ScrollareaContent 的外层容器；进入子节点后 last_node 会变成叶子节点，
    // 再调用 scrollarea_scroll_to 会命中框架的 debug_assert!(false)。
    ctx.scrollarea_scroll_to(Point { x: 0, y: off as CoordType });
    for (i, line) in app.log.iter().enumerate() {
        ctx.next_block_id_mixin(i as u64);
        ctx.styled_label_begin("log-line");
        ctx.styled_label_set_foreground(ctx.indexed(line.kind.color()));
        ctx.styled_label_add_text(&line.text);
        ctx.styled_label_end();
    }
    ctx.scrollarea_end();

    ctx.table_end();
}

fn draw_input(
    ctx: &mut Context,
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
) {
    ctx.table_begin("input-bar");
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    ctx.table_next_row();
    ctx.label("prompt", ">");

    ctx.editline("cmd", &mut app.input);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    // 首帧把焦点交给输入框；之后不抢，菜单展开时焦点留在菜单。
    ctx.focus_on_first_present();

    // 依据当前输入刷新补全候选，并在输入框右侧以灰色显示首个候选的剩余部分。
    app.refresh_completions();
    if let Some(cand) = app.completions.get(app.comp_idx).cloned() {
        let suffix = ghost_suffix(&app.input, &cand);
        if !suffix.is_empty() {
            ctx.styled_label_begin("ghost");
            ctx.styled_label_set_foreground(ctx.indexed(IndexedColor::BrightBlack));
            ctx.styled_label_add_text(&suffix);
            ctx.styled_label_end();
        }
        if app.completions.len() > 1 {
            ctx.label("ghost-count", &format!(" ({})", app.completions.len()));
        }
    }

    if ctx.is_focused() {
        if ctx.consume_shortcut(vk::UP) {
            app.history_prev();
        } else if ctx.consume_shortcut(vk::DOWN) {
            app.history_next();
        } else if ctx.consume_shortcut(vk::TAB) {
            if !app.completions.is_empty() {
                app.accept_completion();
            }
        } else if ctx.consume_shortcut(vk::RETURN) {
            submit(app, run_command, output_drain);
        }
    }

    ctx.table_end();
}

fn draw_status(ctx: &mut Context, app: &App) {
    let who = if app.username.is_empty() { "guest" } else { &app.username };
    let host = if app.server.is_empty() {
        "offline".to_string()
    } else if app.port.is_empty() {
        app.server.clone()
    } else {
        format!("{}:{}", app.server, app.port)
    };
    let freq = app.current_freq.clone().unwrap_or_else(|| "-".to_string());
    let quiet = if app.quiet { "QUIET" } else { "sound" };
    let text = format!(" {who}@{host} | freq {freq} | {quiet} | {}", now_ts());

    ctx.table_begin("status");
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    ctx.attr_background_rgba(ctx.indexed(IndexedColor::Cyan));
    ctx.attr_foreground_rgba(ctx.contrasted(ctx.indexed(IndexedColor::Cyan)));
    ctx.table_next_row();
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    ctx.label("status", &text);
    ctx.table_end();
}

/// 绘制 `connect` 缺配置时的居中询问弹窗（复用框架 modal）。
fn draw_dialog(
    ctx: &mut Context,
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
) {
    if app.dialog.is_none() {
        return;
    }
    let (title, prompt) = match app.dialog.as_ref().unwrap().stage {
        DialogStage::AskServer => ("连接服务器", "服务器地址 [host:port]:"),
        DialogStage::AskUser => ("设置昵称", "昵称:"),
    };

    ctx.modal_begin("connect-dialog", title);
    ctx.label("dlg-prompt", prompt);

    let advance = match app.dialog_buffer() {
        Some(buf) => {
            ctx.editline("dlg-input", buf);
            ctx.focus_on_first_present();
            ctx.is_focused() && ctx.consume_shortcut(vk::RETURN)
        }
        None => false,
    };
    if advance {
        dialog_advance(app, run_command, output_drain);
    }
    let esc = ctx.modal_end();
    if esc {
        app.dialog = None;
    }
}

/// 执行菜单动作。
fn run_action(
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    action: &Action,
) {
    match action {
        Action::Run(argv) => {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            app.note_command(&argv);
            execute_or_dialog(app, run_command, output_drain, &argv);
        }
        Action::Prefill(prefix) => {
            app.input = prefix.to_string();
        }
    }
}

/// 执行命令；若为 `connect` 且缺少 SERVER/USERNAME，改为打开配置弹窗。
fn execute_or_dialog(
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    argv: &[String],
) {
    let wants_connect = argv.first().map(|s| s == "connect").unwrap_or(false);
    if wants_connect && (app.server.is_empty() || app.username.is_empty()) {
        app.open_connect_dialog();
        return;
    }
    run_argv(app, run_command, output_drain, argv);
}

/// 弹窗当前阶段确认：推进到下一阶段，或回填选项后执行 `connect`。
fn dialog_advance(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(d) = app.dialog.as_mut() else { return };
    match d.stage {
        DialogStage::AskServer => {
            if app.username.is_empty() {
                d.stage = DialogStage::AskUser;
            } else {
                finish_dialog(app, run_command, output_drain);
            }
        }
        DialogStage::AskUser => finish_dialog(app, run_command, output_drain),
    }
}

/// 弹窗完成：把收集到的服务器 / 昵称经 `setg` 回填全局选项，再执行 `connect`。
fn finish_dialog(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(d) = app.dialog.take() else { return };
    let server = d.server.trim().to_string();
    let username = d.username.trim().to_string();
    if !server.is_empty() {
        run_argv(
            app,
            run_command,
            output_drain,
            &["setg".to_string(), "SERVER".to_string(), server],
        );
    }
    if !username.is_empty() {
        run_argv(
            app,
            run_command,
            output_drain,
            &["setg".to_string(), "USERNAME".to_string(), username],
        );
    }
    run_argv(app, run_command, output_drain, &["connect".to_string()]);
}

/// 提交输入框内容为命令。
fn submit(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let cmd = app.input.trim().to_string();
    app.input.clear();
    if cmd.is_empty() {
        return;
    }
    app.history.push(cmd.clone());
    app.history_idx = None;
    if matches!(cmd.as_str(), "exit" | "quit" | "q") {
        app.exit = true;
        return;
    }
    let argv: Vec<String> = shell_words::split(&cmd).unwrap_or_else(|_| vec![cmd.clone()]);
    app.note_command(&argv);
    execute_or_dialog(app, run_command, output_drain, &argv);
}

/// 在 TUI 内同步执行命令，输出回显到聊天记录（不退出全屏）。
fn run_argv(
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    argv: &[String],
) {
    let display = argv.join(" ");
    app.push_line(format!("tc> {display}"), LineKind::Cmd);

    let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
        argv.iter().map(|s| s.as_str().into()).collect::<Vec<_>>().into();
    let result = run_command(&args_ffi);
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");

    // 取回 TUI 模式下由 `print_content` 暂存的命令同步输出。
    let drained: Vec<safer_ffi::String> = unsafe { output_drain() }.into();
    let drained: Vec<String> = drained.iter().map(|s| s.to_string()).collect();
    let drained_text = drained.join("\n");

    if result.code != 0 {
        if !result.message.is_empty() {
            app.push_line(result.message.to_string(), LineKind::Result);
        }
        for line in &drained {
            app.push_line(line.clone(), LineKind::Result);
        }
        return;
    }

    for line in &drained {
        app.push_line(line.clone(), LineKind::Result);
    }
    if !result.message.is_empty() {
        app.push_line(result.message.to_string(), LineKind::Result);
    }

    // `freq_info` / `show active_users` 返回成员列表，回填侧栏。
    if argv.iter().any(|a| a == "freq_info" || a == "active_users") {
        app.reconcile_members(&drained_text);
    }
    // 加入 / 创建频点后，把自己记入成员侧栏；他人由 join/leave/msg 推送维护。
    if matches!(cmd, "join_freq" | "create_freq") {
        let me = app.username.clone();
        if !me.is_empty() {
            app.add_member(&me);
        }
    }
    // 离开 / 解散频点后清空成员列表。
    if matches!(cmd, "leave_freq" | "disband") {
        app.members.clear();
    }

    // 命令回显与结果都在末尾，执行后回到最新视图。
    app.follow_latest();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_parses_freq_info_members() {
        let mut app = App::new();
        app.reconcile_members(
            "Freq:    test\nCreator: alice (1.2.3.4)\nMembers: 2/inf\nPassword: no\n  - bob (5.6.7.8)",
        );
        assert_eq!(app.members, vec!["alice".to_string(), "bob".to_string()]);
    }

    #[test]
    fn reconcile_parses_active_users_and_skips_summary() {
        let mut app = App::new();
        app.reconcile_members(
            "  alice   AliceA  @+\n  bob     BobB    away\n2 user(s) in freq `test`",
        );
        assert_eq!(app.members, vec!["AliceA".to_string(), "BobB".to_string()]);
    }

    #[test]
    fn chat_scroll_clamps_at_zero() {
        let mut app = App::new();
        app.scroll_up(5);
        assert!(!app.follow);
        assert_eq!(app.scroll_y, 5);
        app.scroll_down(2);
        assert_eq!(app.scroll_y, 3);
        app.scroll_down(10);
        assert_eq!(app.scroll_y, 0);
        assert!(app.follow);
    }

    #[test]
    fn completes_command_prefix() {
        let mut app = App::new();
        app.input = "con".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "connect"));
    }

    #[test]
    fn completes_member_prefix_after_command() {
        let mut app = App::new();
        app.members = vec!["alice".into(), "bob".into()];
        app.input = "msg al".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["alice".to_string()]);
    }

    #[test]
    fn ghost_suffix_strips_prefix() {
        assert_eq!(ghost_suffix("con", "connect"), "nect");
        assert_eq!(ghost_suffix("msg al", "alice"), "ice");
    }

    #[test]
    fn accept_completion_fills_first_token() {
        let mut app = App::new();
        app.input = "con".into();
        app.refresh_completions();
        let first = app.completions[0].clone();
        app.accept_completion();
        assert_eq!(app.input, first);
    }
}
