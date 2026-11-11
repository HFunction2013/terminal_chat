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
use edit_tui::helpers::{COORD_TYPE_SAFE_MAX, MEBI, Rect, Size};
use edit_tui::input::{self, Input, kbmod, vk};
use edit_tui::sys;
use edit_tui::tui::{Context, Tui};
use edit_tui::vt;

/// 拉取积压推送的 FFI 符号类型。
pub type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

/// 读取全局选项的 FFI 符号类型（cli-core `get_global_option`）。
pub type GetOptionFn =
    unsafe extern "C" fn(&safer_ffi::String) -> TaggedOption<safer_ffi::String>;

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
                MenuItem { label: "Active freqs", action: Action::Run(vec!["show", "active_freqs"]) },
                MenuItem { label: "Active users", action: Action::Run(vec!["show", "active_users"]) },
                MenuItem { label: "Shell…", action: Action::Prefill("shell ") },
            ],
        },
    ]
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
            text: "[tui] 聊天界面：F10/Alt 展开菜单，Enter 执行命令，Ctrl+Q 退出。".to_string(),
            kind: LineKind::System,
        });
        Self {
            log,
            input: String::new(),
            history: Vec::new(),
            history_idx: None,
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
        let kind = display
            .split('[')
            .nth(1)
            .and_then(|s| s.split(']').next())
            .unwrap_or("");
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

    /// 用 `freq_info` 等命令的权威成员列表回填侧栏。
    ///
    /// 支持的行格式：`  - <nick> (<ip>)`、`Creator: <nick> (<ip>)`。
    fn reconcile_members(&mut self, text: &str) {
        let mut names: Vec<String> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            let name = t
                .strip_prefix("- ")
                .or_else(|| t.strip_prefix("Creator: "))
                .and_then(|rest| rest.split(" (").next())
                .map(|s| s.trim().to_string());
            if let Some(n) = name {
                if !n.is_empty() && !names.iter().any(|x| x == &n) {
                    names.push(n);
                }
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
    let res = event_loop(&mut tui, &mut app, &mut vt_parser, &mut input_parser, run_command, take_pushes, get_option, output_drain);

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
            let read_timeout = vt_parser.read_timeout().min(tui.read_timeout()).min(Duration::from_millis(100));
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

    if let Some(key) = ctx.keyboard_input() {
        if key == kbmod::CTRL | vk::Q {
            app.exit = true;
            ctx.set_input_consumed();
        }
    }
}

fn draw_menubar(ctx: &mut Context, app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn, groups: &[MenuGroup]) {
    ctx.menubar_begin();
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

    // 右侧：聊天记录滚动区。
    ctx.scrollarea_begin("chat", Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
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

fn draw_input(ctx: &mut Context, app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    ctx.table_begin("input-bar");
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    ctx.table_next_row();
    ctx.label("prompt", ">");

    let _changed = ctx.editline("cmd", &mut app.input);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    // 首帧把焦点交给输入框；之后不抢，菜单展开时焦点留在菜单。
    ctx.focus_on_first_present();

    if ctx.is_focused() {
        if ctx.consume_shortcut(vk::UP) {
            app.history_prev();
        } else if ctx.consume_shortcut(vk::DOWN) {
            app.history_next();
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

/// 执行菜单动作。
fn run_action(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn, action: &Action) {
    match action {
        Action::Run(argv) => {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            app.note_command(&argv);
            run_argv(app, run_command, output_drain, &argv);
        }
        Action::Prefill(prefix) => {
            app.input = prefix.to_string();
        }
    }
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
    run_argv(app, run_command, output_drain, &argv);
}

/// 在 TUI 内同步执行命令，输出回显到聊天记录（不退出全屏）。
fn run_argv(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn, argv: &[String]) {
    let display = argv.join(" ");
    app.push_line(format!("tc> {display}"), LineKind::Cmd);

    let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> = argv
        .iter()
        .map(|s| s.as_str().into())
        .collect::<Vec<_>>()
        .into();
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

    // `freq_info` 返回权威成员列表，回填侧栏。
    if matches!(cmd, "freq_info") {
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
}
