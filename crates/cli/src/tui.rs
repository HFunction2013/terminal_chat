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
//! │ 状态栏（整行由插件底部回调生成，宿主只渲染）            │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! 框架与 retained-mode 组件树源自 microsoft/edit（MIT，Copyright (c) Microsoft
//! Corporation），已 vendored 到本仓库 `crates/edit-tui`；本模块为聊天客户端界面层。
//!
//! 交互：菜单可用鼠标点击，或 `F10`/`Alt+助记符` 展开；输入框 `Enter` 执行；
//! `Ctrl+Q` 退出。

use crate::ui_registry;
use anyhow::Result;
use cli_core_types::plugin_fn_types::ListRemoteDirFn;
use cli_core_types::{IsConnectedFn, RunCommandFn, SetCurrentFreqFn, SetInterruptedFn};
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use edit_tui::arena::{self, scratch_arena};
use edit_tui::framebuffer::IndexedColor;
use edit_tui::helpers::{COORD_TYPE_SAFE_MAX, CoordType, MEBI, Point, Rect, Size};
use edit_tui::input::{self, Input, kbmod, vk};
use edit_tui::oklab::StraightRgba;
use edit_tui::sys;
use edit_tui::tui::{Anchor, ButtonStyle, Context, FloatSpec, ListSelection, Tui};
use edit_tui::vt;

/// 拉取积压推送的 FFI 符号类型。
pub type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

/// 读取会话/全局选项的 FFI 符号类型（cli-core `get_session_option` /
/// `get_global_option`，两者签名一致）。
pub type GetOptionFn = unsafe extern "C" fn(
    &safer_ffi::String,
    Out<'_, TaggedOption<safer_ffi::String>>,
) -> cli_core_types::Result;

/// 列举已注册选项 key 的 FFI 符号类型（cli-core `get_registered_options`）。
pub type GetRegisteredOptionsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> cli_core_types::Result;

/// 列举顶层命令名的 FFI 符号类型（cli-core `list_commands`）。
pub type ListCommandsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> cli_core_types::Result;

/// 列举指定父命令直接子命令名的 FFI 符号类型（cli-core `list_subcommands`）。
pub type ListSubcommandsFn = unsafe extern "C" fn(
    &safer_ffi::String,
    Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> cli_core_types::Result;

/// 查询命令路径参数规格的 FFI 符号类型（cli-core `arg_spec`）；出参一行 `kind|custom`。
pub type ArgSpecFn = unsafe extern "C" fn(
    &safer_ffi::String,
    &i32,
    Out<'_, safer_ffi::String>,
) -> cli_core_types::Result;

/// 参数补全转发回调（cli-core `complete_arg`）：按首词归属路由到插件 `tc_complete_arg`。
pub type CompleteArgHostFn = unsafe extern "C" fn(
    &safer_ffi::Vec<safer_ffi::String>,
    &i32,
    &safer_ffi::String,
    Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> cli_core_types::Result;

/// 切换 TUI/REPL 输出模式的 FFI 符号类型。
pub type SetTuiModeFn = unsafe extern "C" fn(bool);

/// 取走 TUI 命令输出缓冲的 FFI 符号类型。
pub type OutputDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

/// 查询终端是否正被外部子进程占用（1 占用 / 0 空闲）的 FFI 符号类型。
pub type TerminalBusyFn = unsafe extern "C" fn() -> i32;

/// 后台命令完成后回传 UI 线程的结果。
struct JobResult {
    code: i32,
    message: String,
    argv: Vec<String>,
    /// 该作业是否为后台回填型静默刷新；完成回调据此决定结果消息是否回显。
    silent: bool,
}

/// 常驻命令 worker 的共享句柄。
///
/// 命令在后台线程串行执行（同一时刻只跑一个，避免并发 FFI / 连接竞争），
/// UI 线程只投递 argv、每帧取回实时输出与完成结果，不再被阻塞命令卡住。
#[derive(Clone)]
struct CommandWorker {
    tx: Sender<(Vec<String>, bool)>,
    running: Arc<AtomicBool>,
    result: Arc<Mutex<Option<JobResult>>>,
    /// 当前 / 最近一次提交作业是否为静默回填；跟随作业流，作业完成后不复位，
    /// 直到下一次 submit 覆盖。避免全局单值被相邻作业的完成回调过早翻转。
    silent: Arc<AtomicBool>,
}

impl CommandWorker {
    /// 启动常驻 worker 线程；`run_command` 为裸 FFI 函数指针副本（`Send`/`Copy`）。
    fn spawn(run_command: RunCommandFn) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<(Vec<String>, bool)>();
        let running = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let silent = Arc::new(AtomicBool::new(false));
        let running_w = running.clone();
        let result_w = result.clone();
        thread::spawn(move || {
            while let Ok((argv, silent_flag)) = rx.recv() {
                running_w.store(true, Ordering::SeqCst);
                let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
                    argv.iter().map(|s| s.as_str().into()).collect::<Vec<_>>().into();
                let res = run_command(&args_ffi);
                *result_w.lock().unwrap() = Some(JobResult {
                    code: res.code,
                    message: res.message.to_string(),
                    argv,
                    silent: silent_flag,
                });
                running_w.store(false, Ordering::SeqCst);
            }
        });
        Self { tx, running, result, silent }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 投递待执行命令；`silent` 标记该作业是否为后台回填型刷新。
    fn submit(&self, argv: Vec<String>, silent: bool) {
        self.silent.store(silent, Ordering::SeqCst);
        let _ = self.tx.send((argv, silent));
    }

    /// 当前 / 最近作业是否静默回填（供实时输出回流判定回显归属）。
    fn is_silent(&self) -> bool {
        self.silent.load(Ordering::SeqCst)
    }

    fn take_result(&self) -> Option<JobResult> {
        self.result.lock().unwrap().take()
    }
}

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

/// 侧栏会话快照：一条当前或后台 freq 会话。
#[derive(Clone, PartialEq, Eq)]
struct SessionEntry {
    name: String,
    current: bool,
}

/// 侧栏插件快照：已加载插件的插件名与其命令命名空间。
#[derive(Clone, PartialEq, Eq)]
struct PluginEntry {
    name: String,
    namespace: String,
}

/// 菜单项动作。
#[derive(Clone)]
enum Action {
    /// 直接执行 argv。
    Run(Vec<String>),
    /// 弹出参数表单收集参数；`PREFER_COMMAND_LINE` 为真时退回输入框预填。
    Form(Box<FormSpec>),
}

/// 表单中的一个输入字段。
#[derive(Clone)]
struct FormField {
    /// 输入提示，如 `Frequency ID:`。
    label: String,
    /// 必填：为空时不允许进入下一阶段。
    required: bool,
    /// 密码字段：界面以 `*` 掩码回显，缓冲内仍为明文。
    secret: bool,
    /// 字段类别：决定补全候选来源（命令名 / 成员 / 远端路径 / 本地路径）。
    kind: FieldKind,
}

/// 输入字段的补全类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// 自由文本 / 命令名（默认，不做语义补全）。
    Plain,
    /// 成员名（用户 / 频点内用户名）。
    Member,
    /// 远端路径（经插件 RPC 列举服务端目录）。
    RemotePath,
    /// 本地路径（直接读本机当前目录）。
    LocalPath,
    /// 已注册选项 key（set/setg/get/getg/unset/unsetg 的参数位置）。
    OptionKey,
    /// 复合命令的第二词子命令名（workspace/show/load/... 后）。
    Subcommand,
    /// 已加载插件名（plugin/load/unload 相关参数位置）。
    Plugin,
    /// 已加载插件的命令命名空间（namespace/load/unload 相关参数位置）。
    Namespace,
    /// 插件自定义概念：core 内置类型表未覆盖，经 `complete_arg` 回调向归属插件取候选。
    Custom,
    /// 该位置没有可补全的来源：不弹出任何候选。
    NoCompletion,
}

/// 一个待提交命令的表单规格。
#[derive(Clone)]
struct FormSpec {
    /// 弹窗标题。
    title: String,
    /// 提交后执行的命令名。
    cmd: String,
    /// 命令行偏好下退回输入框的前缀。
    prefill: String,
    fields: Vec<FormField>,
}

#[derive(Clone)]
struct MenuItem {
    label: String,
    action: Action,
}

#[derive(Clone)]
struct MenuGroup {
    name: String,
    accel: char,
    items: Vec<MenuItem>,
}

/// 补全浮层最多同时可见的候选条数；超出部分靠窗口滚动跟随高亮。
const COMPLETION_VIEW: usize = 5;

/// 宿主内置菜单项：退出与信息弹窗命令由本地 `try_builtin` 分发，不进注册表；
/// 其余业务菜单全部由 cli-standard 插件经注册表登记（见 #90 / #150）。
fn builtin_groups() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            name: "File".to_string(),
            accel: 'E',
            items: vec![MenuItem {
                label: "Exit".to_string(),
                action: Action::Run(vec!["exit".to_string()]),
            }],
        },
        MenuGroup {
            name: "Help".to_string(),
            accel: 'H',
            items: vec![
                MenuItem {
                    label: "About".to_string(),
                    action: Action::Run(vec!["about".to_string()]),
                },
                MenuItem {
                    label: "Copyright".to_string(),
                    action: Action::Run(vec!["copyright".to_string()]),
                },
                MenuItem {
                    label: "License".to_string(),
                    action: Action::Run(vec!["license".to_string()]),
                },
                MenuItem {
                    label: "Help".to_string(),
                    action: Action::Run(vec!["help".to_string()]),
                },
            ],
        },
    ]
}

/// 注册表字段类别序号 → 宿主补全类别。
fn reg_field_kind(kind: i32) -> FieldKind {
    use cli_core_types::ui::ui_field_kind as k;
    match kind {
        k::MEMBER => FieldKind::Member,
        k::REMOTE_PATH => FieldKind::RemotePath,
        k::LOCAL_PATH => FieldKind::LocalPath,
        k::OPTION_KEY => FieldKind::OptionKey,
        k::SUBCOMMAND => FieldKind::Subcommand,
        k::PLUGIN => FieldKind::Plugin,
        k::NAMESPACE => FieldKind::Namespace,
        k::NO_COMPLETION => FieldKind::NoCompletion,
        _ => FieldKind::Plain,
    }
}

/// 注册表表单规格 → 宿主表单规格。
fn reg_spec_to_spec(rs: ui_registry::RegFormSpec) -> FormSpec {
    FormSpec {
        title: rs.title,
        cmd: rs.cmd,
        prefill: rs.prefill,
        fields: rs
            .fields
            .into_iter()
            .map(|f| FormField {
                label: f.label,
                required: f.required,
                secret: f.secret,
                kind: reg_field_kind(f.kind),
            })
            .collect(),
    }
}

/// 按 argv 从注册表反查表单规格：先匹配复合命令（前两个 token，如 `workspace switch`），
/// 再匹配单 token 命令；未注册返回 `None`。
fn registry_form_spec(
    argv: &[String],
    form_fn: Option<&ui_registry::GetFormSpecFn>,
) -> Option<FormSpec> {
    if let Some(two) = argv.first_chunk::<2>() {
        let compound = format!("{} {}", two[0], two[1]);
        if let Some(rs) = ui_registry::fetch_form_spec(&compound, form_fn) {
            return Some(reg_spec_to_spec(rs));
        }
    }
    let first = argv.first()?;
    ui_registry::fetch_form_spec(first, form_fn).map(reg_spec_to_spec)
}

/// 注册表菜单动作 → 宿主动作；表单动作按需经查询钩子补全规格。
fn reg_action_to_action(
    action: ui_registry::RegMenuAction,
    form_fn: Option<&ui_registry::GetFormSpecFn>,
) -> Action {
    match action {
        ui_registry::RegMenuAction::Run(argv) => Action::Run(argv),
        ui_registry::RegMenuAction::FormCmd(cmd) => {
            match ui_registry::fetch_form_spec(&cmd, form_fn).map(reg_spec_to_spec) {
                // 规格缺失时降级为直接执行命令，避免菜单条目无响应。
                Some(spec) => Action::Form(Box::new(spec)),
                None => Action::Run(vec![cmd]),
            }
        }
    }
}

/// 注册表菜单组 → 宿主菜单组。
fn reg_groups_to_groups(
    groups: Vec<ui_registry::RegMenuGroup>,
    form_fn: Option<&ui_registry::GetFormSpecFn>,
) -> Vec<MenuGroup> {
    groups
        .into_iter()
        .map(|g| MenuGroup {
            name: g.name,
            accel: g.accel.chars().next().unwrap_or('\0'),
            items: g
                .items
                .into_iter()
                .map(|item| MenuItem {
                    label: item.label,
                    action: reg_action_to_action(item.action, form_fn),
                })
                .collect(),
        })
        .collect()
}

/// 解析当前生效菜单：插件经注册表登记的业务菜单，追加宿主内置的退出 / 信息弹窗组。
fn resolve_groups(
    menu_fn: Option<&ui_registry::GetMenuItemsFn>,
    form_fn: Option<&ui_registry::GetFormSpecFn>,
) -> Vec<MenuGroup> {
    let mut groups = reg_groups_to_groups(ui_registry::fetch_menu_items(menu_fn), form_fn);
    groups.extend(builtin_groups());
    groups
}

/// `connect` 缺配置时的弹窗询问阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogStage {
    /// 先问服务器地址（SERVER[:PORT]）。
    AskServer,
    /// 再问昵称（USERNAME）。
    AskUser,
}

/// 激活中的弹窗。
enum Dialog {
    /// 连接配置：先服务器后昵称，两阶段。
    Connect { stage: DialogStage, server: String, username: String },
    /// 命令参数表单：按字段逐阶段收集。
    Form(FormDialog),
    /// 危险操作二次确认：确认后执行 `argv`，取消则丢弃。
    Confirm { message: String, argv: Vec<String> },
    /// 内置信息展示弹窗：标题 + 多行内容 + Close；内容按弹窗宽度平铺展示。
    Info { title: String, lines: Vec<String> },
}

/// 通用命令表单弹窗状态。
struct FormDialog {
    spec: FormSpec,
    values: Vec<String>,
    focus: usize,
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
    /// 补全浮层窗口首项的全局下标（视口最多 `COMPLETION_VIEW` 条）。
    comp_scroll: usize,
    /// 候选浮层是否处于激活锁定态：激活后 Up/Down 在候选间移动，否则翻历史。
    completion_active: bool,
    /// 鼠标点选候选后，下一帧把焦点交回输入框（点击会先把焦点给到候选项）。
    refocus_input: bool,
    /// 激活中的配置弹窗；`None` 表示无弹窗。
    dialog: Option<Dialog>,
    /// 侧栏快照：当前会话 freq。
    current_freq: Option<String>,
    /// 侧栏快照：收集到的成员名。
    members: Vec<String>,
    /// 侧栏快照：当前 + 后台会话列表（来自 `sessions -l` 输出）。
    sessions: Vec<SessionEntry>,
    /// 待拉取一次会话列表；空闲时投递 `sessions -l`。
    pending_sessions: bool,
    /// 侧栏快照：已加载插件及其命令命名空间（来自 `plugin list` / `show plugins`）。
    plugins: Vec<PluginEntry>,
    /// 待拉取一次插件列表；空闲时投递 `plugin list`。
    pending_plugins: bool,
    /// 插件 / 命名空间装载或卸载后，待从 core 注册表重拉菜单组与侧栏栏目 / 条目模板。
    pending_ui: bool,
    /// 远端目录列举回调（cli-core `remote_list_dir`）；缺失时远端路径不补全。
    remote_list_dir: Option<ListRemoteDirFn>,
    /// 连接态查询回调（cli-core `is_connected`）；缺失时视为未连接。
    is_connected: Option<IsConnectedFn>,
    /// 设置命令中断标志的回调（cli-core `set_interrupted`）；缺失时 Ctrl+C 不中断。
    set_interrupted: Option<SetInterruptedFn>,
    /// 把当前频点写入 cli-core 状态槽的回调（cli-core `set_current_freq`）；
    /// 供插件底部状态栏回调读取；缺失时不写槽。
    set_current_freq: Option<SetCurrentFreqFn>,
    /// 注册表表单规格查询回调（cli-core `get_form_spec`）；命令行回车路径按
    /// 命令名反查已注册表单；缺失时不弹表单。
    get_form_spec: Option<ui_registry::GetFormSpecFn>,
    /// 已注册选项列举回调（cli-core `get_registered_options`）；缺失时选项 key 不补全。
    list_registered_options: Option<GetRegisteredOptionsFn>,
    /// 已注册选项 key 缓存（供选项参数位置补全）。
    registered_options: Vec<String>,
    /// 顶层命令名列举回调（cli-core `list_commands`）；缺失时首词命令不补全。
    list_commands: Option<ListCommandsFn>,
    /// 当前顶层命令名缓存（供首词位置补全）。
    command_names: Vec<String>,
    /// 子命令列举回调（cli-core `list_subcommands`）；缺失时子命令位置不补全。
    list_subcommands: Option<ListSubcommandsFn>,
    /// 参数规格查询回调（cli-core `arg_spec`）；缺失时参数类别退化为不补全。
    arg_spec: Option<ArgSpecFn>,
    /// 自定义概念参数补全转发回调（cli-core `complete_arg`）；缺失时自定义类型不补全。
    complete_arg: Option<CompleteArgHostFn>,
    /// 注册表底部状态栏片段（静态 label + 逐帧回调返回值）；宿主只拼接渲染。
    bottom_fragments: Vec<String>,
    /// 注册表侧栏栏目壳（插件登记）；空时不渲染任何栏目。
    side_sections: Vec<ui_registry::RegSideSection>,
    /// 注册表侧栏条目模板（前缀符号 + 内容占位）；实际条目名由宿主快照回填。
    side_items: Vec<ui_registry::RegSideItem>,
    /// 注册表栏目壳折叠态，按段 id 存取；缺省取段壳 `default_open`。
    side_open: HashMap<String, bool>,
    /// 连接态快照（来自全局 / 会话选项）。
    username: String,
    server: String,
    /// 优先命令行：`PREFER_COMMAND_LINE` 为真值时，表单动作退回输入框预填。
    prefer_cli: bool,
    /// 后台命令 worker；`None` 时退回同步执行（测试桩 / 回退路径）。
    worker: Option<CommandWorker>,
    /// 累积当前后台命令的实时输出文本，命令完成后供成员列表回填。
    worker_output: String,
    /// 当前/最近作业是否为回填型静默刷新（成员/会话/插件列表）：结果只更新侧栏快照，
    /// 不在输出区回显命令行、实时输出或结果消息。
    silent_refresh: bool,
    /// join/create（或重连）成功后待拉取一次当前成员 roster；空闲时投递 `active_users`。
    pending_roster: bool,
    /// 退出标志。
    exit: bool,
    /// 菜单栏 / 状态栏主题背景（对齐 upstream edit 配色）。
    menubar_bg: StraightRgba,
    /// 菜单栏 / 状态栏主题前景。
    menubar_fg: StraightRgba,
}

/// 当前补全上下文：类别、当前前缀、已提交命令路径与参数序号（1 起）。
struct CompletionCtx {
    kind: FieldKind,
    prefix: String,
    /// 已提交的命令路径词（不含正在输入的末词）。
    path: Vec<String>,
    /// 命令后参数序号（1 起）。
    arg_index: i32,
}

impl App {
    fn new() -> Self {
        let mut log = Vec::new();
        log.push(ChatLine {
            text: "[tui] Chat: Up/Down history, Tab complete, Enter accept/run, Esc close popup, F10/Alt menu, Ctrl+Q quit."
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
            comp_scroll: 0,
            completion_active: false,
            refocus_input: false,
            dialog: None,
            current_freq: None,
            members: Vec::new(),
            sessions: Vec::new(),
            pending_sessions: false,
            plugins: Vec::new(),
            pending_plugins: true,
            pending_ui: false,
            remote_list_dir: None,
            is_connected: None,
            set_interrupted: None,
            set_current_freq: None,
            get_form_spec: None,
            list_registered_options: None,
            registered_options: Vec::new(),
            list_commands: None,
            command_names: Vec::new(),
            list_subcommands: None,
            arg_spec: None,
            complete_arg: None,
            bottom_fragments: Vec::new(),
            side_sections: Vec::new(),
            side_items: Vec::new(),
            side_open: HashMap::new(),
            username: String::new(),
            server: String::new(),
            prefer_cli: false,
            worker: None,
            worker_output: String::new(),
            silent_refresh: false,
            pending_roster: false,
            exit: false,
            menubar_bg: StraightRgba::zero(),
            menubar_fg: StraightRgba::zero(),
        }
    }

    fn push_line(&mut self, text: String, kind: LineKind) {
        for line in text.lines() {
            self.log.push(ChatLine { text: line.to_string(), kind });
        }
    }

    /// 回流一条后台命令实时输出：非静默作业回显到输出区；静默回填只累积到
    /// `worker_output` 供侧栏快照 reconcile，不进入输出区。
    fn drain_background(&mut self, text: &str, silent: bool) {
        if !silent {
            self.push_line(text.to_string(), LineKind::Result);
        }
        self.worker_output.push_str(text);
        self.worker_output.push('\n');
    }

    /// Ctrl+C：中断正在运行的后台命令，返回是否消费了该按键。
    ///
    /// 无命令运行时返回 `false`（不退出；退出仍走 Ctrl+Q）。中断标志由命令轮询
    /// 读取，worker 完成后结果照常回流，界面回到可输入态。
    fn interrupt_command(&mut self) -> bool {
        let running = self.worker.as_ref().map(|w| w.is_running()).unwrap_or(false);
        if !running {
            return false;
        }
        self.fire_interrupt();
        true
    }

    /// 置中断标志并在聊天记录提示中断（running 判断之外的副作用，便于测试）。
    fn fire_interrupt(&mut self) {
        if let Some(set_int) = self.set_interrupted {
            let _ = set_int(true);
        }
        self.push_line("^C Interrupting current command.".to_string(), LineKind::System);
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
        names.sort();
        names.dedup();
        if !names.is_empty() && names != self.members {
            self.members = names;
        }
    }

    /// 用 `sessions -l` 输出回填侧栏会话列表快照。
    ///
    /// 支持行格式：
    /// - `  [0] session \`{freq}\` (current)`
    /// - `  [1] session \`{freq}\` (background)`
    /// - `  [0] main console (no freq session)`
    fn reconcile_sessions(&mut self, text: &str) {
        let mut entries: Vec<SessionEntry> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let current = t.contains("(current)");
            // 反引号内为 freq 名；main console 行无反引号，用固定名占位。
            let name = t
                .split('`')
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "main console".to_string());
            if name.is_empty() {
                continue;
            }
            entries.push(SessionEntry { name, current });
        }
        // 当前会话优先，组内按名排序；顺序抖动不触发整表替换重绘。
        entries.sort_by(|a, b| (!a.current, &a.name).cmp(&(!b.current, &b.name)));
        entries.dedup_by(|a, b| a.name == b.name);
        if !entries.is_empty() && entries != self.sessions {
            self.sessions = entries;
        }
    }

    /// 用 `plugin list` / `show plugins` 输出回填侧栏插件快照。
    ///
    /// 行格式：`  {plugin_name} (command: {command_namespace})`；空列表行跳过。
    fn reconcile_plugins(&mut self, text: &str) {
        let mut entries: Vec<PluginEntry> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.contains("no plugins") {
                continue;
            }
            let Some((name, rest)) = t.split_once("(command:") else {
                continue;
            };
            let name = name.trim().to_string();
            let namespace = rest.trim().trim_end_matches(')').trim().to_string();
            if name.is_empty() {
                continue;
            }
            entries.push(PluginEntry { name, namespace });
        }
        // 归一化排序 + 按名去重：注册表 / 输出顺序抖动不应触发整表替换重绘。
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries.dedup_by(|a, b| a.name == b.name);
        if !entries.is_empty() && entries != self.plugins {
            self.plugins = entries;
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
        self.dialog = Some(Dialog::Connect {
            stage,
            server: self.server.clone(),
            username: self.username.clone(),
        });
    }

    /// 打开命令参数表单弹窗：按字段逐阶段收集。
    fn open_form(&mut self, spec: &FormSpec) {
        self.dialog = Some(Dialog::Form(FormDialog {
            spec: spec.clone(),
            values: vec![String::new(); spec.fields.len()],
            focus: 0,
        }));
    }

    /// 打开危险操作二次确认弹窗。
    fn open_confirm(&mut self, message: String, argv: Vec<String>) {
        self.dialog = Some(Dialog::Confirm { message, argv });
    }

    /// 打开内置信息展示弹窗（帮助 / 关于 / 版权 / 许可）。
    fn open_info(&mut self, title: String, lines: Vec<String>) {
        self.dialog = Some(Dialog::Info { title, lines });
    }

    /// 展示内置信息：命令行偏好开启时直接落到终端输出区，否则弹 Info 窗。
    fn present_info(&mut self, title: String, lines: Vec<String>) {
        if self.prefer_cli {
            self.push_line(title, LineKind::Result);
            for line in lines {
                self.push_line(line, LineKind::Result);
            }
        } else {
            self.open_info(title, lines);
        }
    }

    /// 返回弹窗当前阶段正在编辑的文本缓冲。
    fn dialog_buffer(&mut self) -> Option<&mut String> {
        let d = self.dialog.as_mut()?;
        match d {
            Dialog::Connect { stage, server, username } => Some(match stage {
                DialogStage::AskServer => server,
                DialogStage::AskUser => username,
            }),
            Dialog::Form(form) => Some(&mut form.values[form.focus]),
            Dialog::Confirm { .. } | Dialog::Info { .. } => None,
        }
    }

    /// 回到最新消息并恢复跟随。
    fn follow_latest(&mut self) {
        self.follow = true;
        self.scroll_y = 0;
    }

    /// 返回当前正在编辑的缓冲：有弹窗时为弹窗字段，否则为主输入框。
    fn active_buffer(&mut self) -> Option<&mut String> {
        if self.dialog.is_some() { self.dialog_buffer() } else { Some(&mut self.input) }
    }

    /// 经 core 列举某父命令的直接子命令名；回调缺失返回空。
    fn fetch_subcommands(&self, parent: &str) -> Vec<String> {
        let Some(list) = self.list_subcommands else { return Vec::new() };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(&parent.into(), Out::from(&mut buf)) };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    /// 查询命令路径在某参数位置的规格：返回 (内置类别序号, 自定义类型名)。
    fn fetch_arg_spec(&self, path: &str, arg_index: i32) -> (i32, String) {
        use cli_core_types::ui::ui_field_kind as k;
        let Some(query) = self.arg_spec else { return (k::PLAIN, String::new()) };
        let mut spec: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        let _ = unsafe { query(&path.into(), &arg_index, Out::from(&mut spec)) };
        let row = ManuallyDrop::into_inner(spec).to_string();
        match row.split_once('|') {
            Some((kind, custom)) => (kind.trim().parse().unwrap_or(k::PLAIN), custom.to_string()),
            // core 未命中命令 / 参数位置：不补全。
            None => (k::NO_COMPLETION, String::new()),
        }
    }

    /// 判定当前活动缓冲的补全上下文（表单字段类别 / 命令路径与参数序号）。
    fn completion_context(&self) -> CompletionCtx {
        if let Some(Dialog::Form(form)) = &self.dialog {
            return CompletionCtx {
                kind: form.spec.fields[form.focus].kind,
                prefix: form.values[form.focus].clone(),
                path: Vec::new(),
                arg_index: 0,
            };
        }
        let parts: Vec<&str> = self.input.split_whitespace().collect();
        if parts.is_empty() {
            return CompletionCtx {
                kind: FieldKind::Plain,
                prefix: String::new(),
                path: Vec::new(),
                arg_index: 0,
            };
        }
        let trailing = self.input.ends_with(' ');
        let committed: Vec<String>;
        let prefix: String;
        if trailing {
            committed = parts.iter().map(|s| s.to_string()).collect();
            prefix = String::new();
        } else {
            committed = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();
            prefix = parts[parts.len() - 1].to_string();
        }
        if committed.is_empty() {
            // 首词位置：命令名补全。
            return CompletionCtx {
                kind: FieldKind::Plain,
                prefix,
                path: Vec::new(),
                arg_index: 0,
            };
        }
        let cmd = committed[0].clone();
        let arg_index = committed.len() as i32;
        // 第二词位置：父命令若登记了子命令，先补子命令名。
        if committed.len() == 1 && !self.fetch_subcommands(&cmd).is_empty() {
            return CompletionCtx {
                kind: FieldKind::Subcommand,
                prefix,
                path: committed,
                arg_index: 1,
            };
        }
        let path = committed.join(" ");
        let (kind_no, custom) = self.fetch_arg_spec(&path, arg_index);
        let kind = if !custom.is_empty() { FieldKind::Custom } else { reg_field_kind(kind_no) };
        CompletionCtx { kind, prefix, path: committed, arg_index }
    }

    /// 自定义概念参数候选：经 core `complete_arg` 转发到归属插件回调。
    fn custom_candidates(&self, ctx: &CompletionCtx) -> Vec<String> {
        let Some(complete) = self.complete_arg else { return Vec::new() };
        let argv: Vec<safer_ffi::String> = ctx
            .path
            .iter()
            .cloned()
            .chain(std::iter::once(ctx.prefix.clone()))
            .map(|s| s.into())
            .collect();
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe {
            complete(
                &safer_ffi::vec::Vec::from(argv),
                &ctx.arg_index,
                &ctx.prefix.clone().into(),
                Out::from(&mut buf),
            )
        };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    /// 依据当前类别刷新补全候选：命令名 / 子命令 / 成员名 / 路径 / 选项 key / 自定义概念。
    fn refresh_completions(&mut self) {
        self.refresh_command_names();
        let ctx = self.completion_context();
        let cands: Vec<String> = match ctx.kind {
            FieldKind::Plain => {
                // 表单内自由文本不补全；主命令行首词匹配命令名。
                if self.dialog.is_some() {
                    Vec::new()
                } else {
                    self.command_names
                        .iter()
                        .filter(|c| c.starts_with(&ctx.prefix))
                        .cloned()
                        .collect()
                }
            }
            FieldKind::Subcommand => self
                .fetch_subcommands(&ctx.path[0])
                .iter()
                .filter(|s| s.starts_with(&ctx.prefix))
                .cloned()
                .collect(),
            FieldKind::Member => {
                self.members.iter().filter(|m| m.starts_with(&ctx.prefix)).cloned().collect()
            }
            FieldKind::Plugin => self
                .plugins
                .iter()
                .filter(|p| p.name.starts_with(&ctx.prefix))
                .map(|p| p.name.clone())
                .collect(),
            FieldKind::Namespace => self
                .plugins
                .iter()
                .filter(|p| p.namespace.starts_with(&ctx.prefix))
                .map(|p| p.namespace.clone())
                .collect(),
            FieldKind::RemotePath => {
                let (parent, base) = split_path_prefix(&ctx.prefix);
                self.remote_candidates(parent, base)
            }
            FieldKind::LocalPath => {
                let (parent, base) = split_path_prefix(&ctx.prefix);
                local_candidates(parent, base)
            }
            FieldKind::OptionKey => {
                let needle = ctx.prefix.to_uppercase();
                self.registered_options
                    .iter()
                    .filter(|k| k.to_uppercase().starts_with(&needle))
                    .cloned()
                    .collect()
            }
            FieldKind::Custom => self.custom_candidates(&ctx),
            FieldKind::NoCompletion => Vec::new(),
        };
        // 仅当候选集合实际变化时才回到首项并退出选择态；重绘帧保留当前
        // 高亮与视口位置，否则方向键移动高亮后下一帧会被刷回顶部。
        let changed = self.completions != cands;
        self.completions = cands;
        if changed {
            self.comp_idx = 0;
            self.comp_scroll = 0;
            self.completion_active = false;
        } else if self.comp_idx >= self.completions.len() {
            self.comp_idx = 0;
        }
    }

    /// 经插件回调列举远端目录条目，按基名前缀过滤并补回父目录前缀。
    fn remote_candidates(&self, parent: &str, base: &str) -> Vec<String> {
        let Some(list_dir) = self.remote_list_dir else {
            return Vec::new();
        };
        let dir = parent.trim_end_matches('/').to_string();
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = list_dir(&dir.into(), Out::from(&mut buf));
        let entries: Vec<String> =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
        entries
            .into_iter()
            .filter(|n| n.starts_with(base))
            .map(|n| format!("{parent}{n}"))
            .collect()
    }

    /// 从宿主注册表刷新已注册选项 key 缓存（供选项参数位置补全）。
    fn refresh_registered_options(&mut self) {
        let Some(list) = self.list_registered_options else {
            self.registered_options = Vec::new();
            return;
        };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(Out::from(&mut buf)) };
        self.registered_options =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
    }

    /// 从 core 命令表刷新顶层命令名缓存（供首词位置补全）。
    fn refresh_command_names(&mut self) {
        let Some(list) = self.list_commands else {
            self.command_names = Vec::new();
            return;
        };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(Out::from(&mut buf)) };
        self.command_names = ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
    }

    /// 接受当前高亮候选：替换活动缓冲的末词（路径候选已含父目录前缀）。
    /// 侧栏条目双击（或 Enter）：把名称追加到主输入框末尾，空格分隔；
    /// 不替换正在输入的末词。弹窗表单打开时不向主输入框写入。
    fn append_entry_to_input(&mut self, name: &str) {
        if self.dialog.is_some() || name.is_empty() {
            return;
        }
        if !self.input.is_empty() && !self.input.ends_with(' ') {
            self.input.push(' ');
        }
        self.input.push_str(name);
        self.refocus_input = true;
    }

    /// 主命令行下非目录候选接受后补一个空格，进入下一参数位置并刷新该位置补全；
    /// 表单字段为单值，不追加分隔空格。
    fn accept_completion(&mut self) {
        let Some(cand) = self.completions.get(self.comp_idx).cloned() else {
            return;
        };
        let in_dialog = self.dialog.is_some();
        let Some(buf) = self.active_buffer() else {
            return;
        };
        let trimmed = buf.trim_end().to_string();
        let mut parts: Vec<&str> = trimmed.split_whitespace().collect();
        if trimmed.is_empty() {
            *buf = cand.clone();
        } else {
            // 末尾为空格时末词为空（已被 split 丢弃），parts 全是已确认前缀，直接追加；
            // 否则替换正在输入的末词。
            if !buf.ends_with(' ') {
                parts.pop();
            }
            parts.push(cand.as_str());
            *buf = parts.join(" ");
        }
        if !in_dialog && !cand.ends_with('/') {
            buf.push(' ');
        }
        self.completion_active = false;
        self.refresh_completions();
        // 接受后下一帧把焦点交回输入框；弹窗表单内的补全不抢焦点到主输入框。
        if !in_dialog {
            self.refocus_input = true;
        }
    }

    /// 在候选列表中循环切换高亮下标。
    fn cycle_completion(&mut self, delta: isize) {
        let n = self.completions.len();
        if n == 0 {
            return;
        }
        self.comp_idx =
            if delta > 0 { (self.comp_idx + 1) % n } else { (self.comp_idx + n - 1) % n };
        self.clamp_completion_scroll();
    }

    /// 把补全浮层窗口对齐到高亮项：高亮不在视口内时平移窗口，并钳制到
    /// 最后一个可见窗口。视口最多 [`COMPLETION_VIEW`] 条。
    fn clamp_completion_scroll(&mut self) {
        let n = self.completions.len();
        if n == 0 {
            self.comp_scroll = 0;
            return;
        }
        let view = COMPLETION_VIEW.min(n);
        let last = n.saturating_sub(view);
        if self.comp_idx < self.comp_scroll {
            self.comp_scroll = self.comp_idx;
        } else if self.comp_idx >= self.comp_scroll + view {
            self.comp_scroll = self.comp_idx - view + 1;
        }
        self.comp_scroll = self.comp_scroll.min(last);
    }

    /// 当前是否已建立服务器连接；回调缺失或回报 0 时视为未连接。
    fn connected(&self) -> bool {
        let Some(f) = self.is_connected else {
            return false;
        };
        let mut buf: ManuallyDrop<i32> = ManuallyDrop::new(0);
        let _res = f(Out::from(&mut buf));
        ManuallyDrop::into_inner(buf) != 0
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
            "leave_freq" | "disband" | "background" => self.current_freq = None,
            // `sessions -i <freq>` 切换到后台频点；回填目标频点快照。
            "sessions" => {
                let mut it = argv.iter().skip(1);
                while let Some(arg) = it.next() {
                    match arg.as_str() {
                        "-i" | "--interact" => {
                            if let Some(f) = it.next() {
                                self.current_freq = Some(f.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// 判断命令是否需要先建立服务器连接（云端 RPC）。按首词判定，覆盖复合命令
/// （如 `workspace switch`、`load namespace`）；本地命令不在此列。
fn needs_connection(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or(cmd);
    matches!(
        first,
        "join_freq"
            | "create_freq"
            | "msg"
            | "reply"
            | "call"
            | "schedule_msg"
            | "cancel"
            | "mute"
            | "unmute"
            | "ignore"
            | "unignore"
            | "kick"
            | "ban"
            | "unban"
            | "op"
            | "deop"
            | "trop"
            | "user_info"
            | "freq_info"
            | "active_users"
            | "active_freqs"
            | "ls"
            | "cd"
            | "mkdir"
            | "rm"
            | "mv"
            | "cp"
            | "upload"
            | "download"
            | "checksum"
            | "file_info"
            | "tree"
            | "away"
            | "background"
            | "foreground"
            | "leave_freq"
            | "disband"
            | "sessions"
            | "status"
            | "whoami"
            | "pwd"
            | "route"
            | "notifications"
    )
}

/// 判定是否为需要二次确认的危险操作（解散频点、踢人、封禁、删远端文件）。
fn is_dangerous(cmd: &str) -> bool {
    matches!(cmd, "disband" | "kick" | "ban" | "rm")
}

/// 按命令与目标参数生成确认弹窗文案。
fn confirm_message(cmd: &str, argv: &[String]) -> String {
    let target = argv.get(1).cloned().unwrap_or_default();
    match cmd {
        "disband" => "Really disband this frequency? This cannot be undone.".to_string(),
        "kick" => format!("Really kick {target}?"),
        "ban" => format!("Really ban {target}?"),
        "rm" => format!("Really remove remote file {target}?"),
        _ => format!("Really run: {}?", argv.join(" ")),
    }
}

/// 侧栏栏目标题行文本：左标题、右侧状态符号，空格补到侧栏内宽。
/// 侧栏宽 26，左右各 1 格内边距，内宽 24。
fn section_header_text(title: &str, glyph: &str) -> String {
    const SIDEBAR_INNER: usize = 24;
    let used = title.chars().count() + glyph.chars().count();
    let pad = SIDEBAR_INNER.saturating_sub(used).max(1);
    format!("{title}{:pad$}{glyph}", "")
}

/// 拆分路径前缀为 `(父目录, 基名)`；父目录保留尾斜杠，空表示当前目录。
fn split_path_prefix(prefix: &str) -> (&str, &str) {
    match prefix.rfind('/') {
        Some(i) => (&prefix[..=i], &prefix[i + 1..]),
        None => ("", prefix),
    }
}

/// 把 `host:port` 拆成 `(host, 可选 port)`；末段非纯数字时整串视为 host。
fn split_host_port(input: &str) -> (String, Option<String>) {
    let input = input.trim();
    match input.rfind(':') {
        Some(i) => {
            let host = &input[..i];
            let port = &input[i + 1..];
            if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
                return (host.to_string(), Some(port.to_string()));
            }
            (input.to_string(), None)
        }
        None => (input.to_string(), None),
    }
}

/// 列举本机目录条目，按基名前缀过滤；目录条目补 `/` 后缀。
fn local_candidates(parent: &str, base: &str) -> Vec<String> {
    let base_dir = if parent.is_empty() {
        std::env::current_dir().unwrap_or_default()
    } else {
        std::path::PathBuf::from(parent)
    };
    let Ok(rd) = std::fs::read_dir(&base_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(base) {
            continue;
        }
        let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { "/" } else { "" };
        out.push(format!("{parent}{name}{suffix}"));
    }
    // 非根目录时补上上级目录入口，便于向上导航；同样按基名前缀过滤。
    if base_dir.parent().is_some() && "..".starts_with(base) {
        out.push(format!("{parent}../"));
    }
    out.sort();
    out
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

fn opt_get(get: &GetOptionFn, key: &str) -> String {
    let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
        ManuallyDrop::new(TaggedOption::None);
    let _res = unsafe { get(&key.into(), Out::from(&mut buf)) };
    match ManuallyDrop::into_inner(buf) {
        TaggedOption::Some(v) => v.to_string(),
        TaggedOption::None => String::new(),
    }
}

/// 读取有效选项值：会话（session）设置优先，未设置（或为空串）时回退全局。
fn opt_get_effective(
    get_session: Option<&GetOptionFn>,
    get_global: &GetOptionFn,
    key: &str,
) -> String {
    if let Some(session) = get_session {
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let _res = unsafe { session(&key.into(), Out::from(&mut buf)) };
        if let TaggedOption::Some(v) = ManuallyDrop::into_inner(buf) {
            // 会话存了空串视为未设置，避免空值把全局真值盖掉。
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    opt_get(get_global, key)
}

/// 全局选项真值判定：`on/true/1/yes/y/enabled`（大小写、首尾空白不敏感）。
fn is_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_lowercase().as_str(),
        "on" | "true" | "1" | "yes" | "y" | "enabled" | "enable"
    )
}

/// 终端模式恢复：退出时写回 alt screen / 鼠标 / 粘贴模式。
struct RestoreModes;

impl Drop for RestoreModes {
    fn drop(&mut self) {
        sys::write_stdout("\x1b[0 q\x1b[?25h\x1b]0;\x07\x1b[?1000;1002;1006;2004l\x1b[?1049l");
    }
}

/// 运行 TUI 主循环（edit retained-mode 骨架）。
pub fn run(
    run_command: &RunCommandFn,
    take_pushes: &PushDrainFn,
    get_option: &GetOptionFn,
    get_session: Option<&GetOptionFn>,
    set_tui_mode: &SetTuiModeFn,
    output_drain: &OutputDrainFn,
    remote_list_dir: Option<&ListRemoteDirFn>,
    is_connected: Option<&IsConnectedFn>,
    registered_options: Option<&GetRegisteredOptionsFn>,
    list_commands: Option<&ListCommandsFn>,
    list_subcommands: Option<&ListSubcommandsFn>,
    arg_spec: Option<&ArgSpecFn>,
    complete_arg: Option<&CompleteArgHostFn>,
    set_interrupted: Option<&SetInterruptedFn>,
    set_current_freq: Option<&SetCurrentFreqFn>,
    terminal_busy: Option<&TerminalBusyFn>,
    get_menu_items: Option<&ui_registry::GetMenuItemsFn>,
    get_form_spec: Option<&ui_registry::GetFormSpecFn>,
    get_bottom_fragments: Option<&ui_registry::GetBottomFragmentsFn>,
    get_side_sections: Option<&ui_registry::GetSideSectionsFn>,
    get_side_items: Option<&ui_registry::GetSideItemsFn>,
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

    // 主题配色，对齐 upstream microsoft/edit：菜单栏背景由背景色与亮蓝按 1/2
    // 混合，前景取对比色；浮层 / 弹窗由背景 2/3 与前景 1/3 混合。
    let menubar_bg = tui.indexed(IndexedColor::Background).oklab_blend(tui.indexed_alpha(
        IndexedColor::BrightBlue,
        1,
        2,
    ));
    let menubar_fg = tui.contrasted(menubar_bg);
    let floater_bg = tui
        .indexed_alpha(IndexedColor::Background, 2, 3)
        .oklab_blend(tui.indexed_alpha(IndexedColor::Foreground, 1, 3));
    let floater_fg = tui.contrasted(floater_bg);
    tui.set_floater_default_bg(floater_bg);
    tui.set_floater_default_fg(floater_fg);
    tui.set_modal_default_bg(floater_bg);
    tui.set_modal_default_fg(floater_fg);

    // 进 alternate screen、鼠标跟踪、括号粘贴（跳过 OSC 颜色探测，用默认主题）。
    sys::write_stdout("\x1b[?1049h\x1b[?1000;1002;1006;2004h\x1b[?1036h");
    let _restore = RestoreModes;

    let mut app = App::new();
    app.menubar_bg = menubar_bg;
    app.menubar_fg = menubar_fg;
    app.remote_list_dir = remote_list_dir.copied();
    app.is_connected = is_connected.copied();
    app.set_interrupted = set_interrupted.copied();
    app.set_current_freq = set_current_freq.copied();
    app.get_form_spec = get_form_spec.copied();
    app.list_registered_options = registered_options.copied();
    app.list_commands = list_commands.copied();
    app.list_subcommands = list_subcommands.copied();
    app.arg_spec = arg_spec.copied();
    app.complete_arg = complete_arg.copied();
    app.bottom_fragments = ui_registry::fetch_bottom_fragments(get_bottom_fragments)
        .into_iter()
        .map(|f| f.label)
        .collect();
    app.side_sections = ui_registry::fetch_side_sections(get_side_sections);
    app.side_items = ui_registry::fetch_side_items(get_side_items);
    for section in &app.side_sections {
        app.side_open.entry(section.id.clone()).or_insert(section.default_open);
    }

    // 启动后台命令 worker：命令在独立线程串行执行，UI 线程不被阻塞命令卡住；
    // 输出经全局缓冲逐帧回流，完成结果由事件循环取回。
    app.worker = Some(CommandWorker::spawn(*run_command));

    // debug 构建：缺省自动连本地回环并补随机昵称；release 跳过。
    #[cfg(debug_assertions)]
    debug_seed_autoconnect(&mut app, run_command, output_drain, get_option, get_session);

    let res = event_loop(
        &mut tui,
        &mut app,
        &mut vt_parser,
        &mut input_parser,
        run_command,
        take_pushes,
        get_option,
        get_session,
        output_drain,
        terminal_busy,
        get_menu_items,
        get_form_spec,
        get_bottom_fragments,
        get_side_sections,
        get_side_items,
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
    get_session: Option<&GetOptionFn>,
    output_drain: &OutputDrainFn,
    terminal_busy: Option<&TerminalBusyFn>,
    get_menu_items: Option<&ui_registry::GetMenuItemsFn>,
    get_form_spec: Option<&ui_registry::GetFormSpecFn>,
    get_bottom_fragments: Option<&ui_registry::GetBottomFragmentsFn>,
    get_side_sections: Option<&ui_registry::GetSideSectionsFn>,
    get_side_items: Option<&ui_registry::GetSideItemsFn>,
) -> Result<()> {
    let mut groups = resolve_groups(get_menu_items, get_form_spec);
    let mut was_busy = false;

    loop {
        // 刷新连接态快照：会话设置优先，未设置回退全局。
        app.username = opt_get_effective(get_session, get_option, "USERNAME");
        app.server = opt_get_effective(get_session, get_option, "SERVER");
        app.prefer_cli =
            is_truthy(&opt_get_effective(get_session, get_option, "PREFER_COMMAND_LINE"));
        app.refresh_registered_options();
        app.refresh_command_names();

        // 把当前频点写入 cli-core 状态槽，供插件底部状态栏回调读取；再逐帧重取
        // 底部片段（回调按当前状态生成整行文本）。
        if let Some(set_freq) = app.set_current_freq {
            let freq = app.current_freq.clone().unwrap_or_default();
            set_freq(&freq.into());
        }
        app.bottom_fragments = ui_registry::fetch_bottom_fragments(get_bottom_fragments)
            .into_iter()
            .map(|f| f.label)
            .collect();

        // 取积压推送。
        let pushes: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
        for p in pushes {
            let text = p.to_string();
            app.note_push(&text);
            app.push_line(text, LineKind::Push);
        }

        // 后台命令实时输出回流：逐帧取走 `print_content` 暂存的行，累积文本供命令
        // 完成后回填成员 / 侧栏快照。回显归属跟随当前 / 最近作业的静默标记（worker
        // 侧原子），不再读跨帧可被相邻作业翻转的全局单值；无 worker 时回退全局标志。
        let worker_silent =
            app.worker.as_ref().map(|w| w.is_silent()).unwrap_or(app.silent_refresh);
        let drained: Vec<safer_ffi::String> = unsafe { output_drain() }.into();
        for d in drained {
            app.drain_background(&d.to_string(), worker_silent);
        }

        // 后台命令完成：取回结果消息并落地成员侧栏副作用。
        let finished = app.worker.as_ref().and_then(|worker| worker.take_result());
        if let Some(job) = finished {
            apply_job_result(app, job);
        }

        // join/create/重连成功后拉取一次当前成员 roster，补齐加入前已在频点的成员；
        // worker 仍忙则保留标志，下一帧空闲时再投递。
        if app.pending_roster {
            let idle = app.worker.as_ref().map(|w| !w.is_running()).unwrap_or(true);
            if idle {
                app.pending_roster = false;
                run_argv(
                    app,
                    run_command,
                    output_drain,
                    &["show".to_string(), "active_users".to_string()],
                    true,
                );
            }
        }

        // 会话集合变化后拉取一次会话列表，刷新侧栏当前 / 后台会话标注；
        // worker 仍忙则保留标志，下一帧空闲时再投递。
        if app.pending_sessions {
            let idle = app.worker.as_ref().map(|w| !w.is_running()).unwrap_or(true);
            if idle {
                app.pending_sessions = false;
                run_argv(
                    app,
                    run_command,
                    output_drain,
                    &["sessions".to_string(), "-l".to_string()],
                    true,
                );
            }
        }

        // 插件集合变化后拉取一次插件列表，刷新侧栏插件名 / 命名空间分组；
        // worker 仍忙则保留标志，下一帧空闲时再投递。
        if app.pending_plugins {
            let idle = app.worker.as_ref().map(|w| !w.is_running()).unwrap_or(true);
            if idle {
                app.pending_plugins = false;
                run_argv(
                    app,
                    run_command,
                    output_drain,
                    &["plugin".to_string(), "list".to_string()],
                    true,
                );
            }
        }

        // 插件 / 命名空间装载或卸载后，重读 core 注册表：重建菜单组、重拉侧栏栏目 /
        // 条目模板并补齐栏目默认折叠态。worker 仍忙则保留标志，下一帧空闲时再刷新。
        if app.pending_ui {
            let idle = app.worker.as_ref().map(|w| !w.is_running()).unwrap_or(true);
            if idle {
                app.pending_ui = false;
                app.side_sections = ui_registry::fetch_side_sections(get_side_sections);
                app.side_items = ui_registry::fetch_side_items(get_side_items);
                for section in &app.side_sections {
                    app.side_open.entry(section.id.clone()).or_insert(section.default_open);
                }
                groups = resolve_groups(get_menu_items, get_form_spec);
            }
        }

        // 外部 TUI 子进程占用终端期间：不读按键、不画帧、不写终端输出，
        // 把终端完全交给子进程；busy 复位后下一帧整屏重绘。
        let busy = terminal_busy.map(|f| unsafe { f() != 0 }).unwrap_or(false);

        // 刚从外部 TUI 子进程恢复：期间终端屏被子进程覆盖，内存帧仍是挂起前的
        // 旧帧，增量 diff 会误判整行未变而跳过输出。下一帧强制整屏重绘。
        if was_busy && !busy {
            tui.force_full_redraw();
        }
        was_busy = busy;

        // 读取一批输入（超时较短，便于定时拉取推送）。
        if !busy {
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
        if !busy {
            // 定时刷新：即使本轮无按键 / resize，也重建一帧节点树，把回流的推送、
            // 实时输出、完成结果画进界面。否则新内容只改了 App 状态，要等下一次
            // 输入事件触发 draw 才会进入节点树并被 render 输出。
            draw(tui, None, app, run_command, output_drain, &groups);
            while tui.needs_settling() {
                draw(tui, None, app, run_command, output_drain, &groups);
            }
        }

        if app.exit {
            break;
        }

        if !busy {
            let scratch = scratch_arena(None);
            let output = tui.render(&scratch);
            sys::write_stdout(&output);
        }
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

    // 滚轮归属：只有鼠标悬停在候选浮层内时才滚动候选；悬停在聊天输出区则
    // 翻聊天记录。弹窗打开时聊天记录不响应滚轮。
    // 框架约定：`y < 0` 向上滚（高亮上移），`y > 0` 向下滚（高亮下移）。
    let wheel = ctx.scroll_delta();
    let hovering_suggestions = !app.completions.is_empty() && ctx.hover_in_classname("suggestions");
    if app.dialog.is_none() && hovering_suggestions {
        if wheel.y < 0 {
            app.completion_active = true;
            app.cycle_completion(-1);
            ctx.set_input_consumed();
        } else if wheel.y > 0 {
            app.completion_active = true;
            app.cycle_completion(1);
            ctx.set_input_consumed();
        }
    } else if app.dialog.is_none() {
        if wheel.y < 0 {
            app.scroll_up((-wheel.y) as usize);
            ctx.set_input_consumed();
        } else if wheel.y > 0 {
            app.scroll_down(wheel.y as usize);
            ctx.set_input_consumed();
        }
    }

    // 对齐 upstream microsoft/edit：菜单栏、主体、输入栏、状态栏作为 root 下
    // 纵向堆叠的三段，不套外层边框；弹窗 / 浮层在其后叠加。
    draw_menubar(ctx, app, run_command, output_drain, groups);
    draw_body(ctx, app);
    draw_input(ctx, app, run_command, output_drain);
    draw_status(ctx, app);
    draw_dialog(ctx, app, run_command, output_drain);

    if let Some(key) = ctx.keyboard_input() {
        if key == kbmod::CTRL | vk::Q {
            app.exit = true;
            ctx.set_input_consumed();
        } else if key == kbmod::CTRL | vk::C {
            // 仅中断正在运行的后台命令；无命令运行时不退出（退出仍走 Ctrl+Q）。
            if app.interrupt_command() {
                ctx.set_input_consumed();
            }
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
    ctx.attr_background_rgba(app.menubar_bg);
    ctx.attr_foreground_rgba(app.menubar_fg);
    // 菜单栏撑满整行宽度，与下方主体 / 输入框 / 状态栏左右边缘对齐。
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    let contains_focus = ctx.contains_focus();
    for group in groups {
        if ctx.menubar_menu_begin(group.name.as_str(), group.accel) {
            for item in &group.items {
                let clicked = ctx.menubar_menu_button(item.label.as_str(), '\0', vk::NULL);
                if clicked {
                    run_action(app, run_command, output_drain, &item.action);
                }
            }
            ctx.menubar_menu_end();
        }
    }
    if !contains_focus && ctx.consume_shortcut(vk::F10) {
        ctx.steal_focus();
    }
    ctx.menubar_end();
}

/// 计算聊天记录滚动起始行偏移。
///
/// 框架 scrollarea 以 `scroll_offset.y` 定位：`0` 为最早一行，`base`（总行数
/// 减视口高度）为贴最新一行。回看历史时 `scroll_y` 增大，偏移从底部向顶部
/// 回退，并在顶部钳制到 0。
fn chat_offset(total: usize, view: usize, follow: bool, scroll_y: usize) -> usize {
    let base = total.saturating_sub(view);
    if follow { base } else { base.saturating_sub(scroll_y) }
}

fn draw_body(ctx: &mut Context, app: &mut App) {
    // 内部纵向：菜单栏(1) + 主体 + 输入框(1) + 状态栏(1)。
    let body_height = (ctx.size().height - 3).max(1);

    ctx.table_begin("body");
    ctx.table_set_columns(&[26, COORD_TYPE_SAFE_MAX]);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
    ctx.table_next_row();

    // 左侧：会话 / 成员。整个侧栏共用一个滚动区，栏目各自可折叠。
    ctx.block_begin("sidebar");
    ctx.attr_padding(Rect::two(0, 1));
    ctx.attr_intrinsic_size(Size { width: 26, height: body_height });

    ctx.scrollarea_begin("sidebar-scroll", Size { width: 26, height: body_height });

    draw_sidebar_registry(ctx, app);

    ctx.scrollarea_end();
    ctx.block_end();

    // 右侧：聊天记录滚动区。全量喂入，由框架 scrollarea 处理滚轮 / 滚动条 /
    // 翻页；`follow` 时贴底，否则按回看行数上移。
    let total = app.log.len();
    let view = body_height.max(0) as usize;
    let off = chat_offset(total, view, app.follow, app.scroll_y);

    ctx.scrollarea_begin("chat", Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
    // 与侧栏共用左右各 1 格内边距：聊天文字不贴双栏分界，左右对称
    // （滚动条仍由框架在最右列自动预留）。
    ctx.attr_padding(Rect::two(0, 1));
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

/// 注册表驱动侧栏：遍历插件登记的栏目壳，按 `source` 取宿主快照，模板回填条目外观。
fn draw_sidebar_registry(ctx: &mut Context, app: &mut App) {
    let sections = app.side_sections.clone();
    for (i, section) in sections.iter().enumerate() {
        let open = *app.side_open.get(&section.id).unwrap_or(&section.default_open);
        let glyph = if open { "#" } else { ">" };
        ctx.next_block_id_mixin(i as u64);
        let clicked = ctx.row_button(
            "sidebar-reg-section",
            &section_header_text(&section.title, glyph),
            app.menubar_bg,
            app.menubar_fg,
        );
        let open = if clicked { !open } else { open };
        app.side_open.insert(section.id.clone(), open);
        if !open {
            continue;
        }
        let templates: Vec<ui_registry::RegSideItem> =
            app.side_items.iter().filter(|item| item.section == section.id).cloned().collect();
        match section.source.as_str() {
            "sessions" => render_sessions_template(ctx, app, &templates),
            "members" => render_members_template(ctx, app, &templates),
            "plugins" => render_plugins_template(ctx, app, &templates),
            _ => {
                ctx.label("none", "  (none)");
            }
        }
    }
}

/// 回填模板占位：`{name}` ↔ 条目名，`{namespace}` ↔ 命名空间；未声明占位原样保留。
fn fill_side_label(label: &str, name: &str, namespace: &str) -> String {
    label.replace("{name}", name).replace("{namespace}", namespace)
}

/// 按主 / 次行与高亮标记选取模板；高亮无匹配时回退到该行任意模板。
fn find_template(
    templates: &[ui_registry::RegSideItem],
    line: i32,
    highlight: bool,
) -> Option<&ui_registry::RegSideItem> {
    templates
        .iter()
        .find(|t| t.line == line && t.highlight == highlight)
        .or_else(|| templates.iter().find(|t| t.line == line))
}

/// 会话快照：当前会话套用高亮模板整行绿色；其余按普通模板缩进列出。
fn render_sessions_template(
    ctx: &mut Context,
    app: &mut App,
    templates: &[ui_registry::RegSideItem],
) {
    if app.sessions.is_empty() {
        let freq = app.current_freq.clone().unwrap_or_else(|| "(not joined)".to_string());
        ctx.label("freq", &format!("  {freq}"));
        return;
    }
    ctx.list_begin("sidebar-sessions");
    let sessions = app.sessions.clone();
    for s in &sessions {
        let name = s.name.clone();
        let tpl = find_template(templates, 0, s.current);
        let (prefix, text, highlight) = match tpl {
            Some(t) => {
                (t.prefix.clone(), fill_side_label(&t.label, &name, ""), t.highlight && s.current)
            }
            None => ("  ".to_string(), name.clone(), s.current),
        };
        let sel = if highlight {
            ctx.styled_list_item_begin();
            ctx.styled_label_set_foreground(ctx.indexed(IndexedColor::Green));
            ctx.styled_label_add_text(&format!("{prefix}{text}"));
            ctx.styled_list_item_end(false)
        } else {
            ctx.list_item(false, &format!("{prefix}{text}"))
        };
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }
    }
    ctx.list_end();
}

/// 成员快照：按成员模板回填条目名；空快照显示 `(none)`。
fn render_members_template(
    ctx: &mut Context,
    app: &mut App,
    templates: &[ui_registry::RegSideItem],
) {
    if app.members.is_empty() {
        ctx.label("none", "  (none)");
        return;
    }
    ctx.list_begin("sidebar-members");
    let members = app.members.clone();
    for m in &members {
        let name = m.clone();
        let text = match find_template(templates, 0, false) {
            Some(t) => format!("{}{}", t.prefix, fill_side_label(&t.label, &name, "")),
            None => format!("  {name}"),
        };
        let sel = ctx.list_item(false, &text);
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }
    }
    ctx.list_end();
}

/// 插件快照：主行插件名，次行命令命名空间；双击分别插入对应名字。
fn render_plugins_template(
    ctx: &mut Context,
    app: &mut App,
    templates: &[ui_registry::RegSideItem],
) {
    if app.plugins.is_empty() {
        ctx.label("plugin-none", "  (none)");
        return;
    }
    ctx.list_begin("sidebar-plugins");
    let plugins = app.plugins.clone();
    for p in &plugins {
        let name = p.name.clone();
        let ns = p.namespace.clone();
        let primary = match find_template(templates, 0, false) {
            Some(t) => format!("{}{}", t.prefix, fill_side_label(&t.label, &name, "")),
            None => format!("  • {name}"),
        };
        let sel = ctx.list_item(false, &primary);
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }

        let secondary = match find_template(templates, 1, false) {
            Some(t) => format!("{}{}", t.prefix, fill_side_label(&t.label, "", &ns)),
            None => format!("  : {ns}"),
        };
        let sel2 = ctx.list_item(false, &secondary);
        if let ListSelection::Activated = sel2 {
            app.append_entry_to_input(&ns);
        }
    }
    ctx.list_end();
}

fn draw_input(
    ctx: &mut Context,
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
) {
    ctx.table_begin("input-bar");
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    // 与侧栏 / 状态栏共用左右各 1 格内边距，提示符与下方状态栏左缘对齐。
    ctx.attr_padding(Rect::two(0, 1));
    ctx.table_next_row();
    ctx.label("prompt", ">");

    ctx.editline("cmd", &mut app.input);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    // 首帧把焦点交给输入框；之后不抢，菜单展开时焦点留在菜单。
    ctx.focus_on_first_present();
    // 鼠标点选候选后，焦点曾被候选项抢走；这里交回输入框行容器。
    if app.refocus_input {
        app.refocus_input = false;
        ctx.steal_focus();
    }

    // 依据当前输入刷新补全候选。
    app.refresh_completions();

    // 按键判断必须放在候选浮层挂载之前：浮层 list 会被表格包成第二个
    // 兄弟行，list_end 后 last_node 停在浮层行，而焦点在含输入框的兄弟行，
    // 此时再判断子树归属会失配导致 Enter 不触发提交。当前节点仍是包含
    // 输入框的行容器，用 contains_focus 判断子树是否聚焦。
    if ctx.contains_focus() {
        if ctx.consume_shortcut(vk::UP) {
            if app.completion_active && !app.completions.is_empty() {
                app.cycle_completion(-1);
            } else {
                app.history_prev();
            }
        } else if ctx.consume_shortcut(vk::DOWN) {
            if app.completion_active && !app.completions.is_empty() {
                app.cycle_completion(1);
            } else {
                app.history_next();
            }
        } else if ctx.consume_shortcut(vk::TAB) {
            if !app.completions.is_empty() {
                // Tab 直接接受当前高亮候选并填入输入框。
                app.accept_completion();
            }
        } else if ctx.consume_shortcut(vk::ESCAPE) {
            app.completion_active = false;
        } else if ctx.consume_shortcut(vk::RETURN) {
            if app.completion_active && !app.completions.is_empty() {
                app.accept_completion();
            } else {
                submit(app, run_command, output_drain);
            }
        }
    }

    // 候选非空时叠加补全浮层；弹窗打开时由 `draw_dialog` 负责弹窗内浮层，
    // 主输入框不再重复绘制一份。
    if app.dialog.is_none() {
        draw_suggestions(ctx, app);
    }

    ctx.table_end();
}

/// 候选非空时，在当前控件下方叠加一个浮层列表（对齐 upstream edit 的
/// autocomplete suggestions 浮层），当前高亮项随 `comp_idx` 标记。
fn draw_suggestions(ctx: &mut Context, app: &mut App) {
    if app.completions.is_empty() {
        return;
    }
    let bg = ctx.indexed_alpha(IndexedColor::Background, 3, 4);
    let fg = ctx.contrasted(bg);
    ctx.list_begin("suggestions");
    ctx.attr_float(FloatSpec {
        anchor: Anchor::Last,
        gravity_x: 0.0,
        gravity_y: 1.0,
        offset_x: 0.0,
        offset_y: 0.0,
    });
    ctx.attr_border();
    ctx.attr_background_rgba(bg);
    ctx.attr_foreground_rgba(fg);
    let cands = app.completions.clone();
    let n = cands.len();
    let view = COMPLETION_VIEW.min(n);
    let start = app.comp_scroll.min(n.saturating_sub(view));
    let end = (start + view).min(n);
    for (local, cand) in cands[start..end].iter().enumerate() {
        let idx = start + local;
        let selected = app.completion_active && idx == app.comp_idx;
        let sel = ctx.list_item(selected, cand);
        match sel {
            ListSelection::Activated => {
                app.comp_idx = idx;
                app.accept_completion();
            }
            ListSelection::Selected => {
                app.comp_idx = idx;
                app.completion_active = true;
            }
            ListSelection::Unchanged => {}
        }
    }
    app.clamp_completion_scroll();
    ctx.list_end();
}

fn draw_status(ctx: &mut Context, app: &App) {
    // 整行文本由插件底部回调按当前状态生成（静态片段 + 逐帧回调返回值）；
    // 宿主只负责拼接渲染，不再内置拼接 who@host / freq / quiet / 时间戳。
    let text = app.bottom_fragments.join(" | ");

    ctx.table_begin("status");
    ctx.attr_focus_well();
    ctx.attr_background_rgba(app.menubar_bg);
    ctx.attr_foreground_rgba(app.menubar_fg);
    ctx.table_set_cell_gap(Size { width: 2, height: 0 });
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
    ctx.attr_padding(Rect::two(0, 1));
    ctx.table_next_row();
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

    // 内置信息展示弹窗：纯文本 + Close，无文本输入。
    if let Dialog::Info { title, lines } = app.dialog.as_ref().unwrap() {
        ctx.modal_begin("info-dialog", title);
        ctx.block_begin("content");
        ctx.inherit_focus();
        ctx.attr_padding(Rect::three(1, 2, 1));

        // 内容逐行平铺展示，不进滚动区；同一父节点下每行 classname 相同，
        // 用 `next_block_id_mixin` 区分节点 ID。
        for (i, line) in lines.iter().enumerate() {
            ctx.next_block_id_mixin(i as u64);
            ctx.label("info-line", line);
        }

        let closed = ctx.button("info-close", "Close", ButtonStyle::default());
        ctx.block_end();
        if closed {
            app.dialog = None;
        }
        let esc = ctx.modal_end();
        if esc {
            app.dialog = None;
        }
        return;
    }

    // 危险操作确认框：纯按钮，无文本输入。
    if let Dialog::Confirm { message, .. } = app.dialog.as_ref().unwrap() {
        ctx.modal_begin("confirm-dialog", "Confirm");
        ctx.block_begin("content");
        ctx.inherit_focus();
        ctx.attr_padding(Rect::three(1, 2, 1));
        ctx.label("confirm-msg", message);
        ctx.label("confirm-hint", "Enter: confirm    Esc: cancel");
        let confirmed =
            ctx.button("confirm-ok", "Confirm", ButtonStyle::default().bracketed(false));
        ctx.block_end();
        if confirmed {
            dialog_advance(app, run_command, output_drain);
        }
        let esc = ctx.modal_end();
        if esc {
            app.dialog = None;
        }
        return;
    }

    let (title, prompt, secret) = match app.dialog.as_ref().unwrap() {
        Dialog::Connect { stage, .. } => match stage {
            DialogStage::AskServer => ("Connect", "Server [host:port]:", false),
            DialogStage::AskUser => ("Nickname", "Nickname:", false),
        },
        Dialog::Form(form) => {
            let field = &form.spec.fields[form.focus];
            (form.spec.title.as_str(), field.label.as_str(), field.secret)
        }
        Dialog::Confirm { .. } => unreachable!("confirm dialog rendered above"),
        Dialog::Info { .. } => unreachable!("info dialog rendered above"),
    };

    ctx.modal_begin("connect-dialog", title);
    ctx.block_begin("content");
    ctx.inherit_focus();
    ctx.attr_padding(Rect::three(1, 2, 1));

    ctx.label("dlg-prompt", prompt);

    let advance = match app.dialog_buffer() {
        Some(buf) => {
            if secret {
                ctx.editline_password("dlg-input", buf);
            } else {
                ctx.editline("dlg-input", buf);
            }
            ctx.focus_on_first_present();
            let focused = ctx.is_focused();
            app.refresh_completions();
            if focused {
                if ctx.consume_shortcut(vk::UP) {
                    app.cycle_completion(-1);
                } else if ctx.consume_shortcut(vk::DOWN) {
                    app.cycle_completion(1);
                } else if ctx.consume_shortcut(vk::TAB) {
                    if !app.completions.is_empty() {
                        app.accept_completion();
                    }
                }
            }
            focused && ctx.consume_shortcut(vk::RETURN)
        }
        None => false,
    };
    ctx.block_end();

    draw_suggestions(ctx, app);

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
            app.note_command(argv);
            execute_or_dialog(app, run_command, output_drain, argv);
        }
        Action::Form(spec) => {
            if needs_connection(spec.cmd.as_str()) && !app.connected() {
                app.push_line("Not connected. Run `connect` first.".to_string(), LineKind::Result);
            } else if app.prefer_cli {
                // 优先命令行：退回输入框预填前缀，交用户补全参数；
                // 同时清掉残留表单弹窗，避免浮层挡住输入框。
                app.dialog = None;
                app.input = spec.prefill.clone();
            } else {
                app.open_form(spec);
            }
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
    // 宿主内置命令（退出 / 清屏 / 版本信息等）在本地拦截，不进插件 FFI；
    // 否则 cli-core 会给未注册命令加 `std` 前缀，clap 匹配不到子命令而报错。
    if try_builtin(app, argv) {
        return;
    }
    let wants_connect = argv.first().map(|s| s == "connect").unwrap_or(false);
    if wants_connect && (app.server.is_empty() || app.username.is_empty()) {
        app.open_connect_dialog();
        return;
    }
    // 需连接的云端命令：未连接时直接提示，不发 RPC、不弹表单。
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
    if needs_connection(cmd) && !app.connected() {
        app.push_line("Not connected. Run `connect` first.".to_string(), LineKind::Result);
        return;
    }
    // 带参命令在命令行偏好关闭、且未带任何参数时，打开参数表单收集；
    // 已敲参数则按原样执行。命令行偏好开启时保持直接走命令行（见 #51 反向口径）。
    if !app.prefer_cli
        && argv.len() <= 1
        && let Some(spec) = registry_form_spec(argv, app.get_form_spec.as_ref())
    {
        app.open_form(&spec);
        return;
    }
    // 危险操作：先弹二次确认框，确认后再执行。
    if is_dangerous(cmd) {
        app.open_confirm(confirm_message(cmd, argv), argv.to_vec());
        return;
    }
    run_argv(app, run_command, output_drain, argv, false);
}

/// REPL 同款内置命令在 TUI 内的本地分发。
///
/// 返回 `true` 表示已本地处理，调用方无需再走 `run_command` FFI。
fn try_builtin(app: &mut App, argv: &[String]) -> bool {
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "exit" | "quit" | "q" => {
            app.exit = true;
            true
        }
        "clear" => {
            app.log.clear();
            true
        }
        "version" => {
            let lines = crate::welcome_text(false).lines().map(String::from).collect();
            app.present_info("About".to_string(), lines);
            true
        }
        "build_info" => {
            let lines = crate::build_info_text().lines().map(String::from).collect();
            app.present_info("Build Info".to_string(), lines);
            true
        }
        "about" => {
            let mut lines: Vec<String> =
                crate::welcome_text(false).lines().map(String::from).collect();
            lines.push(String::new());
            lines.extend(crate::build_info_text().lines().map(String::from));
            app.present_info("About".to_string(), lines);
            true
        }
        "copyright" => {
            app.present_info("Copyright".to_string(), vec![crate::copyright_text()]);
            true
        }
        "license" => {
            let lines: Vec<String> = crate::license_text().lines().map(String::from).collect();
            app.present_info("License".to_string(), lines);
            true
        }
        "coffee" => {
            app.push_line(COFFEE_ART.to_string(), LineKind::Result);
            true
        }
        "saying" => match crate::saying_text() {
            Some(msg) => {
                app.push_line(msg, LineKind::Result);
                true
            }
            None => {
                app.push_line("Error choosing saying".to_string(), LineKind::System);
                true
            }
        },
        "help" => {
            let lines: Vec<String> =
                crate::builtin_help_lines().into_iter().map(String::from).collect();
            app.present_info("Help".to_string(), lines);
            true
        }
        // 终端动画命令依赖 REPL 的清屏 / 帧循环，全屏 TUI 内不适用。
        "banner" | "train" => {
            app.push_line(
                format!(
                    "`{cmd}` is an animated terminal command; run it in the REPL, not the TUI."
                ),
                LineKind::System,
            );
            true
        }
        // `sleep` 占用事件循环，不适合在全屏 TUI 内运行；编辑器经后台 worker
        // 拉起，前后由终端钩子离开 / 重进 alt 屏（见 #70）。
        "sleep" => {
            app.push_line(
                "`sleep` blocks the TUI event loop; run it in the REPL, not the TUI.".to_string(),
                LineKind::System,
            );
            true
        }
        _ => false,
    }
}

const COFFEE_ART: &str = r"                  ( (
                   ) )
                 ........
                 |      |]
                 \      /
                  `----'
Because everyone deserves a good cup of coffee.";

/// 弹窗当前阶段确认：推进到下一阶段，或回填选项后执行 `connect`。
fn dialog_advance(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(d) = app.dialog.as_mut() else { return };
    match d {
        Dialog::Connect { stage, username, .. } => match stage {
            DialogStage::AskServer => {
                if username.is_empty() {
                    *stage = DialogStage::AskUser;
                } else {
                    finish_dialog(app, run_command, output_drain);
                }
            }
            DialogStage::AskUser => finish_dialog(app, run_command, output_drain),
        },
        Dialog::Form(form) => {
            let field = &form.spec.fields[form.focus];
            let value = form.values[form.focus].trim();
            // 必填字段留空时停留在当前字段，不推进。
            if field.required && value.is_empty() {
                return;
            }
            if form.focus + 1 < form.spec.fields.len() {
                form.focus += 1;
            } else {
                finish_form(app, run_command, output_drain);
            }
        }
        Dialog::Confirm { .. } => finish_confirm(app, run_command, output_drain),
        Dialog::Info { .. } => {}
    }
}

/// 确认框通过：执行原待提交 argv。
fn finish_confirm(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(Dialog::Confirm { argv, .. }) = app.dialog.take() else { return };
    app.note_command(&argv);
    run_argv(app, run_command, output_drain, &argv, false);
}

/// 生成 `hf` + 两位随机小写字母的调试昵称（如 `hfqx`）。
#[cfg(debug_assertions)]
fn debug_random_nickname() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let suffix: String = (0..2).map(|_| (b'a' + rng.gen_range(0..26)) as char).collect();
    format!("hf{suffix}")
}

/// 计算 debug 启动种子要执行的命令：缺省才补本地回环与随机昵称，最后连接。
#[cfg(debug_assertions)]
fn debug_seed_argv(server: &str, username: &str) -> Vec<Vec<String>> {
    let mut cmds: Vec<Vec<String>> = Vec::new();
    if server.is_empty() {
        cmds.push(vec![
            "setg".to_string().into(),
            "SERVER".to_string().into(),
            "127.0.0.1".to_string().into(),
        ]);
        cmds.push(vec![
            "setg".to_string().into(),
            "PORT".to_string().into(),
            "50051".to_string().into(),
        ]);
    }
    if username.is_empty() {
        let nick = debug_random_nickname();
        cmds.push(vec!["setg".to_string().into(), "USERNAME".to_string().into(), nick]);
    }
    cmds.push(vec!["connect".to_string().into()]);
    cmds
}

/// debug 构建启动种子：缺省自动连本地回环 `127.0.0.1:50051`，并补一个随机昵称。
/// 已显式设置的 `SERVER` / `USERNAME` 不被覆盖；release 构建不调用本函数。
#[cfg(debug_assertions)]
fn debug_seed_autoconnect(
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    get_option: &GetOptionFn,
    get_session: Option<&GetOptionFn>,
) {
    let server = opt_get_effective(get_session, get_option, "SERVER");
    let username = opt_get_effective(get_session, get_option, "USERNAME");
    for argv in debug_seed_argv(&server, &username) {
        run_argv(app, run_command, output_drain, &argv, false);
    }
}

/// 弹窗完成：把收集到的服务器 / 昵称经 `setg` 回填全局选项，再执行 `connect`。
fn finish_dialog(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(d) = app.dialog.take() else { return };
    let (server, username) = match d {
        Dialog::Connect { server, username, .. } => (server, username),
        Dialog::Form(_) | Dialog::Confirm { .. } | Dialog::Info { .. } => return,
    };
    if !server.is_empty() {
        let (host, port) = split_host_port(&server);
        if !host.is_empty() {
            run_argv(
                app,
                run_command,
                output_drain,
                &["setg".to_string(), "SERVER".to_string(), host],
                false,
            );
        }
        if let Some(p) = port {
            run_argv(
                app,
                run_command,
                output_drain,
                &["setg".to_string(), "PORT".to_string(), p],
                false,
            );
        }
    }
    if !username.is_empty() {
        run_argv(
            app,
            run_command,
            output_drain,
            &["setg".to_string(), "USERNAME".to_string(), username],
            false,
        );
    }
    run_argv(app, run_command, output_drain, &["connect".to_string()], false);
}

/// 按命令名把表单字段值组装成 argv（纯函数，便于单测）。
fn form_argv(spec: &FormSpec, vals: &[String]) -> Vec<String> {
    let trimmed: Vec<String> = vals.iter().map(|s| s.trim().to_string()).collect();
    let opt = |i: usize| trimmed.get(i).filter(|s| !s.is_empty()).cloned();

    match spec.cmd.as_str() {
        "join_freq" => {
            let mut argv = vec!["join_freq".to_string()];
            if let Some(freq) = opt(0) {
                argv.push(freq);
            }
            if let Some(pw) = opt(1) {
                argv.push(pw);
            }
            argv
        }
        "workspace switch" => {
            let mut argv = vec!["workspace".to_string().into(), "switch".to_string().into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            argv
        }
        "workspace new" => {
            let mut argv = vec!["workspace".to_string().into(), "new".to_string().into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            argv
        }
        "create_freq" => {
            let mut argv = vec!["create_freq".to_string().into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            if let Some(pw) = opt(1) {
                argv.push(pw);
            }
            argv
        }
        "msg" => {
            let mut argv = vec!["msg".to_string().into()];
            if let Some(users) = opt(0) {
                argv.extend(shell_words::split(&users).unwrap_or_else(|_| vec![users]));
            }
            if let Some(msg) = opt(1) {
                argv.push("-m".into());
                argv.push(msg);
            }
            argv
        }
        "reply" => {
            let mut argv = vec!["reply".to_string().into()];
            if let Some(msg) = opt(0) {
                argv.push("-m".into());
                argv.push(msg);
            }
            argv
        }
        "call" | "mute" | "unmute" | "ignore" | "kick" | "ban" | "rm" | "unban" | "unignore"
        | "op" | "deop" | "trop" | "user_info" | "mkdir" | "cd" | "checksum" | "file_info"
        | "cancel" | "get" | "getg" | "unset" | "unsetg" => {
            let mut argv = vec![spec.cmd.clone()];
            if let Some(a) = opt(0) {
                argv.push(a);
            }
            argv
        }
        "ls" => {
            let mut argv = vec!["ls".to_string().into()];
            if let Some(d) = opt(0) {
                argv.push(d);
            }
            argv
        }
        "freq_info" => {
            let mut argv = vec!["freq_info".to_string().into()];
            if let Some(f) = opt(0) {
                argv.push(f);
            }
            argv
        }
        "set" => {
            let mut argv = vec!["set".to_string().into()];
            if let Some(k) = opt(0) {
                argv.push(k);
            }
            if let Some(v) = opt(1) {
                argv.push(v);
            }
            argv
        }
        "setg" => {
            let mut argv = vec!["setg".to_string().into()];
            if let Some(k) = opt(0) {
                argv.push(k);
            }
            if let Some(v) = opt(1) {
                argv.push(v);
            }
            argv
        }
        "schedule_msg" => {
            let mut argv = vec!["schedule_msg".to_string().into()];
            if let Some(time) = opt(0) {
                argv.push(time);
            }
            if let Some(users) = opt(2) {
                argv.extend(shell_words::split(&users).unwrap_or_else(|_| vec![users]));
            }
            if let Some(msg) = opt(1) {
                argv.push("-m".into());
                argv.push(msg);
            }
            argv
        }
        "upload" => {
            let mut argv = vec!["upload".to_string().into()];
            if let Some(local) = opt(0) {
                argv.push(local);
            }
            if let Some(remote) = opt(1) {
                argv.push(remote);
            }
            argv
        }
        "download" => {
            let mut argv = vec!["download".to_string().into()];
            if let Some(remote) = opt(0) {
                argv.push(remote);
            }
            if let Some(local) = opt(1) {
                argv.push(local);
            }
            argv
        }
        "mv" | "cp" => {
            let mut argv = vec![spec.cmd.clone()];
            if let Some(a) = opt(0) {
                argv.push(a);
            }
            if let Some(b) = opt(1) {
                argv.push(b);
            }
            argv
        }
        "load" => {
            let mut argv = vec!["load".to_string().into(), "plugin".to_string().into()];
            if let Some(p) = opt(0) {
                argv.push(p);
            }
            argv
        }
        "unload" => {
            let mut argv = vec!["unload".to_string().into(), "plugin".to_string().into()];
            if let Some(p) = opt(0) {
                argv.push(p);
            }
            argv
        }
        "load namespace" => {
            let mut argv = vec!["load".to_string().into(), "namespace".to_string().into()];
            if let Some(ns) = opt(0) {
                argv.push(ns);
            }
            argv
        }
        "unload namespace" => {
            let mut argv = vec!["unload".to_string().into(), "namespace".to_string().into()];
            if let Some(ns) = opt(0) {
                argv.push(ns);
            }
            argv
        }
        "shell" => {
            let mut argv = vec!["shell".to_string().into()];
            if let Some(cmd) = opt(0) {
                argv.extend(shell_words::split(&cmd).unwrap_or_else(|_| vec![cmd]));
            }
            argv
        }
        _ => vec![spec.cmd.clone()],
    }
}

/// 表单完成：按命令名把收集到的字段组装成 argv 并执行。
fn finish_form(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(Dialog::Form(form)) = app.dialog.take() else { return };
    let vals: Vec<String> = form.values.iter().map(|s| s.trim().to_string()).collect();
    let argv = form_argv(&form.spec, &vals);
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
    if is_dangerous(cmd) {
        app.open_confirm(confirm_message(cmd, &argv), argv);
        return;
    }
    app.note_command(&argv);
    run_argv(app, run_command, output_drain, &argv, false);
}

/// 提交输入框内容为命令。
fn submit(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let cmd = app.input.trim().to_string();
    app.input.clear();
    app.completion_active = false;
    if cmd.is_empty() {
        return;
    }
    app.history.push(cmd.clone());
    app.history_idx = None;
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
    background: bool,
) {
    // 静默只作用于后台自动回填投递；用户主动敲入的命令（即使是列表查询）
    // 仍正常回显到输出区。
    let silent = background && is_silent_refresh(argv);

    // 有后台 worker：投递到 worker 线程串行执行，不阻塞 UI；实时输出由事件循环
    // 每帧 drain 回流，完成结果在事件循环里取回（见 `apply_job_result`）。
    if let Some(worker) = app.worker.clone() {
        // worker 仍在跑上一条作业：本次投递被拒，保留在跑作业的静默归属，
        // 不用新命令的口径覆盖全局标志。
        if worker.is_running() {
            if !silent {
                app.push_line(
                    "A command is still running; wait for it to finish.".to_string(),
                    LineKind::System,
                );
            }
            return;
        }
        // 入队前清走上一条命令残留在 FFI 输出缓冲的行；残留并入回填文本，
        // 不回显到输出区，避免跨命令帧间泄漏。
        let drained: Vec<safer_ffi::String> = unsafe { output_drain() }.into();
        let leftover: Vec<String> = drained.iter().map(|s| s.to_string()).collect();
        app.worker_output.clear();
        for line in &leftover {
            app.worker_output.push_str(line);
            app.worker_output.push('\n');
        }
        app.silent_refresh = silent;
        if !silent {
            let display = argv.join(" ");
            app.push_line(format!("tc> {display}"), LineKind::Cmd);
        }
        worker.submit(argv.to_vec(), silent);
        app.follow_latest();
        return;
    }

    // 无 worker（测试桩 / 回退路径）：保持旧的同步执行。
    app.silent_refresh = silent;
    if !silent {
        let display = argv.join(" ");
        app.push_line(format!("tc> {display}"), LineKind::Cmd);
    }
    run_argv_sync(app, run_command, output_drain, argv, silent);
}

/// 同步执行命令（worker 缺失时的回退路径，行为与历史一致）。
fn run_argv_sync(
    app: &mut App,
    run_command: &RunCommandFn,
    output_drain: &OutputDrainFn,
    argv: &[String],
    silent: bool,
) {
    let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
        argv.iter().map(|s| s.as_str().into()).collect::<Vec<_>>().into();
    let result = run_command(&args_ffi);

    // 取回 TUI 模式下由 `print_content` 暂存的命令同步输出。
    let drained: Vec<safer_ffi::String> = unsafe { output_drain() }.into();
    let drained: Vec<String> = drained.iter().map(|s| s.to_string()).collect();
    let drained_text = drained.join("\n");

    if result.code != 0 {
        if !silent {
            if !result.message.is_empty() {
                app.push_line(result.message.to_string(), LineKind::Result);
            }
            for line in &drained {
                app.push_line(line.clone(), LineKind::Result);
            }
        }
        app.follow_latest();
        return;
    }

    if !silent {
        for line in &drained {
            app.push_line(line.clone(), LineKind::Result);
        }
        if !result.message.is_empty() {
            app.push_line(result.message.to_string(), LineKind::Result);
        }
    }

    apply_member_side_effects(app, argv, &drained_text, result.code == 0);
    app.follow_latest();
}

/// 判断命令是否为回填型后台刷新：结果已由对应侧栏栏目展示，不在输出区重复回显
/// 命令行、实时输出或结果消息（输出仍累积到 `worker_output` 供快照回填）。
fn is_silent_refresh(argv: &[String]) -> bool {
    let mut parts = argv.iter().map(|s| s.as_str());
    match parts.next() {
        Some("show") => matches!(parts.next(), Some("active_users") | Some("plugins")),
        Some("plugin") => parts.next() == Some("list"),
        Some("sessions") => argv.iter().any(|a| a == "-l" || a == "--list" || a == "list"),
        _ => false,
    }
}

/// 命令完成后对成员侧栏的本地回填/清空（同步路径与后台完成回调共用）。
/// `success` 为命令返回码是否为 0；只有成功才触发后续 roster 拉取。
fn apply_member_side_effects(app: &mut App, argv: &[String], drained_text: &str, success: bool) {
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
    // `freq_info` / `show active_users` 返回成员列表，回填侧栏。
    if argv.iter().any(|a| a == "freq_info" || a == "active_users") {
        app.reconcile_members(drained_text);
    }
    let has_interact = argv.windows(2).any(|w| matches!(w[0].as_str(), "-i" | "--interact"));
    let has_kill = argv.windows(2).any(|w| matches!(w[0].as_str(), "-k" | "--kill"));
    // `sessions -l` 返回会话列表，回填侧栏会话快照（切换 / 丢弃子命令不回填）。
    if cmd == "sessions" && !has_interact && !has_kill {
        app.reconcile_sessions(drained_text);
    }
    // `plugin list` / `show plugins` 返回插件列表，回填侧栏插件名 / 命名空间分组。
    let is_plugin_list = (cmd == "plugin" && argv.iter().any(|a| a == "list"))
        || (cmd == "show" && argv.iter().any(|a| a == "plugins"));
    if is_plugin_list {
        app.reconcile_plugins(drained_text);
    }
    // 插件 / 命名空间装载或卸载成功后，重拉插件列表刷新侧栏分组，并重读 core
    // 注册表刷新菜单组与侧栏栏目 / 条目模板（卸载插件注册项已在 core 侧注销）。
    if success && matches!(cmd, "plugin" | "namespace" | "load" | "unload") {
        app.pending_plugins = true;
        app.pending_ui = true;
    }
    // 加入 / 创建频点后，把自己记入成员侧栏；他人由 join/leave/msg 推送维护。
    if matches!(cmd, "join_freq" | "create_freq") {
        let me = app.username.clone();
        if !me.is_empty() {
            app.add_member(&me);
        }
        // 成功后再拉取一次当前 roster，补齐加入前已在频点的成员（不只靠后续推送）。
        if success {
            app.pending_roster = true;
            app.pending_sessions = true;
        }
    }
    // 重连成功后同样拉取一次 roster，恢复此前所在频点的成员快照。
    if success && cmd == "connect" {
        app.pending_roster = true;
        app.pending_sessions = true;
    }
    // 离开 / 解散频点后清空成员列表，并刷新会话列表。
    if matches!(cmd, "leave_freq" | "disband") {
        app.members.clear();
        app.pending_roster = false;
        app.pending_sessions = true;
    }
    // `sessions -i <freq>` 切换到后台频点：清掉上一个频点的残留成员，成功后
    // 重拉目标频点 roster；不清空会在侧栏留下旧频点成员。
    if cmd == "sessions" {
        if has_interact && success {
            app.members.clear();
            app.pending_roster = true;
        }
        if (has_interact || has_kill) && success {
            app.pending_sessions = true;
        }
    }
    // 挂起 / 恢复后台会话后会话集合变化，刷新会话列表。
    if success && matches!(cmd, "background" | "foreground") {
        app.pending_sessions = true;
    }
    // 改成员集合 / 权限标记的管理命令成功后重拉 roster：被踢 / 被封者从侧栏
    // 移除，op/deop/trop 的权限标记同步刷新。mute/ignore 不改成员集合，不拉。
    if success && matches!(cmd, "kick" | "ban" | "unban" | "op" | "deop" | "trop") {
        app.pending_roster = true;
    }
}

/// 后台命令完成：取回结果消息并落地成员侧栏副作用。
fn apply_job_result(app: &mut App, job: JobResult) {
    let drained_text = app.worker_output.clone();
    app.worker_output.clear();

    // 结果消息是否回显以该作业自身的静默归属为准，不依赖可能被后续命令覆盖的全局标志。
    if !job.silent {
        if job.code != 0 {
            if !job.message.is_empty() {
                app.push_line(job.message, LineKind::Result);
            }
        } else if !job.message.is_empty() {
            app.push_line(job.message, LineKind::Result);
        }
    }

    apply_member_side_effects(app, &job.argv, &drained_text, job.code == 0);
    app.silent_refresh = false;
    app.follow_latest();
}

#[cfg(test)]
mod tests {
    use super::*;

    static RUN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    extern "C" fn stub_run(_: &safer_ffi::Vec<safer_ffi::String>) -> cli_core_types::Result {
        RUN_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        cli_core_types::Result::default()
    }
    extern "C" fn stub_connected(out: Out<'_, i32>) -> cli_core_types::Result {
        out.write(1);
        cli_core_types::Result::success()
    }
    unsafe extern "C" fn stub_drain() -> safer_ffi::vec::Vec<safer_ffi::String> {
        Vec::new().into()
    }
    const STUB_RUN: RunCommandFn = stub_run;
    const STUB_DRAIN: OutputDrainFn = stub_drain;
    const STUB_CONNECTED: IsConnectedFn = stub_connected;

    /// 注册表表单规格桩：仅返回 `STUB_FORM_CMDS` 内登记命令的桩规格。
    static STUB_FORM_CMDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    extern "C" fn stub_form_spec(
        cmd: &safer_ffi::String,
        out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let registered = STUB_FORM_CMDS.lock().unwrap().clone();
        let rows: Vec<safer_ffi::String> = if registered.iter().any(|c| c == &cmd.to_string()) {
            vec![format!("SPEC|Stub|{cmd}|{cmd} ").into(), "FIELD|Value:|1|0|0".into()]
        } else {
            Vec::new()
        };
        out.write(rows.into());
        cli_core_types::Result::success()
    }
    const STUB_FORM_SPEC: ui_registry::GetFormSpecFn = stub_form_spec;

    /// 测试用字段桩：自由文本类别。
    fn stub_field(required: bool, secret: bool) -> FormField {
        FormField { label: String::new(), required, secret, kind: FieldKind::Plain }
    }

    /// 测试用表单规格桩：标题留空，仅携带命令名 / 前缀 / 字段。
    fn stub_spec(cmd: &str, prefill: &str, fields: Vec<FormField>) -> FormSpec {
        FormSpec {
            title: String::new(),
            cmd: cmd.to_string(),
            prefill: prefill.to_string(),
            fields,
        }
    }

    static INTERRUPT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    extern "C" fn stub_set_interrupted(_: bool) -> cli_core_types::Result {
        INTERRUPT_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        cli_core_types::Result::success()
    }
    const STUB_SET_INT: SetInterruptedFn = stub_set_interrupted;

    static SESSION_OPT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    static GLOBAL_OPT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    // 上述桩为共享静态：相关用例须串行，避免并行交错覆盖取值。
    static OPT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    extern "C" fn stub_session_option(
        _: &safer_ffi::String,
        out: Out<'_, TaggedOption<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let val = SESSION_OPT.lock().unwrap().clone();
        out.write(val.map(|v| v.into()).into());
        cli_core_types::Result::success()
    }
    extern "C" fn stub_global_option(
        _: &safer_ffi::String,
        out: Out<'_, TaggedOption<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let val = GLOBAL_OPT.lock().unwrap().clone();
        out.write(val.map(|v| v.into()).into());
        cli_core_types::Result::success()
    }
    const STUB_SESSION: GetOptionFn = stub_session_option;
    const STUB_GLOBAL: GetOptionFn = stub_global_option;

    // ---- 命令表 / 子命令 / 参数规格 / 自定义概念补全桩（对齐 core 查询符号） ----

    static STUB_COMMANDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    extern "C" fn stub_list_commands(
        out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let rows: Vec<safer_ffi::String> =
            STUB_COMMANDS.lock().unwrap().iter().map(|s| s.clone().into()).collect();
        out.write(rows.into());
        cli_core_types::Result::success()
    }
    const STUB_LIST_COMMANDS: ListCommandsFn = stub_list_commands;

    static STUB_SUBS: std::sync::Mutex<Vec<(String, Vec<String>)>> =
        std::sync::Mutex::new(Vec::new());
    extern "C" fn stub_list_subcommands(
        parent: &safer_ffi::String,
        out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let subs = STUB_SUBS
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == &parent.to_string())
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let rows: Vec<safer_ffi::String> = subs.iter().map(|s| s.clone().into()).collect();
        out.write(rows.into());
        cli_core_types::Result::success()
    }
    const STUB_SUBS_FN: ListSubcommandsFn = stub_list_subcommands;

    /// 参数规格桩表：`(命令路径, 参数序号, 内置类别序号, 自定义类型名)`。
    static STUB_ARG_SPEC: std::sync::Mutex<Vec<(String, i32, i32, String)>> =
        std::sync::Mutex::new(Vec::new());
    extern "C" fn stub_arg_spec(
        path: &safer_ffi::String,
        arg_index: &i32,
        out: Out<'_, safer_ffi::String>,
    ) -> cli_core_types::Result {
        let table = STUB_ARG_SPEC.lock().unwrap().clone();
        let parts: Vec<&str> = path.split_whitespace().collect();
        // 模拟 core 最长前缀截断：在表内命令路径中取与入参前缀匹配最长的一条。
        let mut best: Option<(String, usize)> = None;
        for entry in &table {
            let key_parts: Vec<&str> = entry.0.split_whitespace().collect();
            if parts.starts_with(&key_parts[..]) {
                match &best {
                    Some((_, d)) if *d >= key_parts.len() => {}
                    _ => best = Some((entry.0.clone(), key_parts.len())),
                }
            }
        }
        let row = match best {
            Some((key, depth)) => {
                let param = arg_index - depth as i32 + 1;
                table
                    .iter()
                    .find(|(p, i, _, _)| p == &key && *i == param)
                    .map(|(_, _, k, c)| format!("{k}|{c}"))
                    .unwrap_or_else(|| "8|".to_string())
            }
            None => "8|".to_string(),
        };
        out.write(row.into());
        cli_core_types::Result::success()
    }
    const STUB_ARG_SPEC_FN: ArgSpecFn = stub_arg_spec;

    static STUB_CUSTOM: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    extern "C" fn stub_complete_arg(
        _argv: &safer_ffi::Vec<safer_ffi::String>,
        _arg_index: &i32,
        prefix: &safer_ffi::String,
        out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let needle = prefix.to_string();
        let rows: Vec<safer_ffi::String> = STUB_CUSTOM
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .filter(|s| s.starts_with(&needle))
            .map(|s| s.into())
            .collect();
        out.write(rows.into());
        cli_core_types::Result::success()
    }
    const STUB_COMPLETE_ARG: CompleteArgHostFn = stub_complete_arg;

    /// 给 App 装入命令表 / 子命令 / 参数规格 / 自定义补全桩。
    fn stub_command_table(app: &mut App) {
        app.list_commands = Some(STUB_LIST_COMMANDS);
        app.list_subcommands = Some(STUB_SUBS_FN);
        app.arg_spec = Some(STUB_ARG_SPEC_FN);
        app.complete_arg = Some(STUB_COMPLETE_ARG);
    }

    fn set_session(v: Option<&str>) {
        *SESSION_OPT.lock().unwrap() = v.map(|s| s.to_string());
    }
    fn set_global(v: Option<&str>) {
        *GLOBAL_OPT.lock().unwrap() = v.map(|s| s.to_string());
    }

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
    fn reconcile_parses_session_list_current_and_background() {
        let mut app = App::new();
        app.reconcile_sessions("  [0] session `1.0` (current)\n  [1] session `0.1` (background)");
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.sessions[0].name, "1.0");
        assert!(app.sessions[0].current);
        assert_eq!(app.sessions[1].name, "0.1");
        assert!(!app.sessions[1].current);
    }

    #[test]
    fn reconcile_parses_main_console_session() {
        let mut app = App::new();
        app.reconcile_sessions("  [0] main console (no freq session)");
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].name, "main console");
        assert!(!app.sessions[0].current);
    }

    #[test]
    fn reconcile_sessions_empty_text_keeps_snapshot() {
        let mut app = App::new();
        app.reconcile_sessions("  [0] session `1.0` (current)");
        app.reconcile_sessions("");
        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn reconcile_plugins_parses_name_and_namespace() {
        let mut app = App::new();
        app.reconcile_plugins("  cli-standard (command: std)\n  extra (command: xns)");
        assert_eq!(app.plugins.len(), 2);
        assert_eq!(app.plugins[0].name, "cli-standard");
        assert_eq!(app.plugins[0].namespace, "std");
        assert_eq!(app.plugins[1].name, "extra");
        assert_eq!(app.plugins[1].namespace, "xns");
    }

    #[test]
    fn reconcile_plugins_normalizes_order_and_dedupes() {
        let mut app = App::new();
        // 输入顺序颠倒：归一化后按名排序，快照顺序稳定。
        app.reconcile_plugins("  extra (command: xns)\n  cli-standard (command: std)");
        assert_eq!(app.plugins[0].name, "cli-standard");
        assert_eq!(app.plugins[1].name, "extra");
        // 重复条目去重，不产生重复快照。
        app.reconcile_plugins(
            "  extra (command: xns)\n  extra (command: xns)\n  cli-standard (command: std)",
        );
        assert_eq!(app.plugins.len(), 2);
    }

    #[test]
    fn reconcile_plugins_empty_text_keeps_snapshot() {
        let mut app = App::new();
        app.reconcile_plugins("  cli-standard (command: std)");
        app.reconcile_plugins("");
        assert_eq!(app.plugins.len(), 1);
    }

    #[test]
    fn append_entry_to_input_adds_space_separated_name() {
        let mut app = App::new();
        app.append_entry_to_input("alice");
        assert_eq!(app.input, "alice");
        app.append_entry_to_input("bob");
        assert_eq!(app.input, "alice bob");
        assert!(app.refocus_input);
    }

    #[test]
    fn append_entry_to_input_keeps_trailing_space() {
        let mut app = App::new();
        app.input = "kick ".to_string();
        app.append_entry_to_input("carol");
        assert_eq!(app.input, "kick carol");
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
    fn chat_offset_follows_bottom_then_backtracks() {
        // 20 行、视口 8：贴底偏移 = 12。
        assert_eq!(chat_offset(20, 8, true, 0), 12);
        // 回看 3 行：偏移从底部向顶部回退到 9。
        assert_eq!(chat_offset(20, 8, false, 3), 9);
        // 回看超过历史：钳制到顶部 0。
        assert_eq!(chat_offset(20, 8, false, 20), 0);
        // 日志不足一屏：始终在顶部。
        assert_eq!(chat_offset(3, 8, true, 0), 0);
        assert_eq!(chat_offset(3, 8, false, 5), 0);
        // 空日志：不产生负偏移。
        assert_eq!(chat_offset(0, 8, false, 5), 0);
    }

    #[test]
    fn chat_offset_backtrack_decreases_not_increases() {
        // 回归：回看历史必须减小偏移（向顶部回退），而不是从底部继续增大。
        let bottom = chat_offset(20, 8, true, 0);
        let reviewed = chat_offset(20, 8, false, 4);
        assert!(reviewed < bottom);
        assert_eq!(reviewed, bottom - 4);
    }

    #[test]
    fn completes_command_prefix() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() =
            vec!["connect".to_string(), "status".to_string(), "unload".to_string()];
        app.input = "con".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "connect"));
    }

    #[test]
    fn completes_member_prefix_after_command() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![("msg".to_string(), 1, 1, String::new())];
        app.members = vec!["alice".to_string().into(), "bob".to_string().into()];
        app.input = "msg al".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["alice".to_string()]);
    }

    #[test]
    fn completes_plugin_name_after_plugin_load_or_unload() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![
            ("plugin load".to_string(), 1, 6, String::new()),
            ("plugin unload".to_string(), 1, 6, String::new()),
        ];
        app.plugins = vec![
            PluginEntry {
                name: "cli-standard".to_string().into(),
                namespace: "std".to_string().into(),
            },
            PluginEntry { name: "extra".to_string().into(), namespace: "xns".to_string().into() },
        ];
        app.input = "plugin load cl".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["cli-standard".to_string()]);

        app.input = "plugin unload ex".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["extra".to_string()]);
    }

    #[test]
    fn completes_namespace_after_namespace_load_or_unload() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![
            ("namespace load".to_string(), 1, 7, String::new()),
            ("namespace unload".to_string(), 1, 7, String::new()),
        ];
        app.plugins = vec![
            PluginEntry {
                name: "cli-standard".to_string().into(),
                namespace: "std".to_string().into(),
            },
            PluginEntry { name: "extra".to_string().into(), namespace: "xns".to_string().into() },
        ];
        app.input = "namespace load x".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["xns".to_string()]);

        app.input = "namespace unload st".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["std".to_string()]);
    }

    #[test]
    fn arg_spec_routes_to_plugin_namespace_member_option() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![
            ("plugin load".to_string(), 1, 6, String::new()),
            ("namespace load".to_string(), 1, 7, String::new()),
            ("msg".to_string(), 1, 1, String::new()),
            ("set".to_string(), 1, 4, String::new()),
            ("set".to_string(), 2, 8, String::new()),
        ];
        let mut kind_of = |input: &str| {
            app.input = input.into();
            app.completion_context().kind
        };
        assert_eq!(kind_of("plugin load x"), FieldKind::Plugin);
        assert_eq!(kind_of("namespace load x"), FieldKind::Namespace);
        assert_eq!(kind_of("msg a"), FieldKind::Member);
        assert_eq!(kind_of("set pre"), FieldKind::OptionKey);
        assert_eq!(kind_of("set k v"), FieldKind::NoCompletion);
    }

    #[test]
    fn no_command_names_after_command_without_arg_source() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        // 参数规格桩表为空：未命中位置落 NO_COMPLETION，不回退命令名候选。
        app.input = "status ".into();
        app.refresh_completions();
        assert!(
            app.completions.is_empty(),
            "no command names after a no-arg command: {:?}",
            app.completions
        );

        app.input = "route ".into();
        app.refresh_completions();
        assert!(app.completions.is_empty());
    }

    #[test]
    fn command_name_completion_only_at_first_word() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() =
            vec!["connect".to_string(), "status".to_string(), "unload".to_string()];
        app.input = "sta".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "status"));
    }

    #[test]
    fn unload_is_in_first_word_command_completion() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() =
            vec!["connect".to_string(), "status".to_string(), "unload".to_string()];
        app.input = "un".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "unload"));
    }

    #[test]
    fn completes_registered_option_prefix_case_insensitive() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![("set".to_string(), 1, 4, String::new())];
        app.registered_options = vec![
            "SERVER".to_string().into(),
            "PORT".to_string().into(),
            "PREFER_COMMAND_LINE".to_string().into(),
        ];
        app.input = "set pre".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["PREFER_COMMAND_LINE".to_string()]);
    }

    #[test]
    fn completion_view_shows_all_when_fewer_than_view() {
        let mut app = App::new();
        app.completions =
            vec!["a".to_string().into(), "b".to_string().into(), "c".to_string().into()];
        app.clamp_completion_scroll();
        assert_eq!(app.comp_scroll, 0);
    }

    #[test]
    fn completion_window_follows_highlight_down() {
        let mut app = App::new();
        app.completions = (0..8).map(|i| format!("c{i}")).collect();
        // 下移到第 6 项（idx 5），窗口平移到 1。
        for _ in 0..5 {
            app.cycle_completion(1);
        }
        assert_eq!(app.comp_idx, 5);
        assert_eq!(app.comp_scroll, 1);
        // 到最后一项 idx 7，窗口钳到 last = 8 - 5 = 3。
        for _ in 0..2 {
            app.cycle_completion(1);
        }
        assert_eq!(app.comp_idx, 7);
        assert_eq!(app.comp_scroll, 3);
    }

    #[test]
    fn completion_window_follows_highlight_up() {
        let mut app = App::new();
        app.completions = (0..8).map(|i| format!("c{i}")).collect();
        app.comp_idx = 7;
        app.comp_scroll = 3;
        // 上移到 idx 2：高亮落出窗口顶部，窗口回卷到 2。
        for _ in 0..5 {
            app.cycle_completion(-1);
        }
        assert_eq!(app.comp_idx, 2);
        assert_eq!(app.comp_scroll, 2);
        // 继续上移到 idx 0，窗口贴顶 0。
        for _ in 0..2 {
            app.cycle_completion(-1);
        }
        assert_eq!(app.comp_idx, 0);
        assert_eq!(app.comp_scroll, 0);
    }

    #[test]
    fn accept_completion_fills_first_token() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() = vec!["connect".to_string()];
        app.input = "con".into();
        app.refresh_completions();
        let first = app.completions[0].clone();
        app.accept_completion();
        // 主命令行接受首词候选后补空格，进入第一参数位置。
        assert_eq!(app.input, format!("{first} "));
    }

    #[test]
    fn completes_subcommand_after_compound_prefix() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_SUBS.lock().unwrap() =
            vec![("workspace".to_string(), vec!["list".to_string(), "new".to_string()])];
        app.members = vec!["alice".to_string().into()];
        app.input = "workspace l".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "list"));
        // 第二词位置不应回退到成员名补全。
        assert!(!app.completions.iter().any(|c| c == "alice"));
    }

    #[test]
    fn completes_all_subcommands_after_trailing_space() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_SUBS.lock().unwrap() =
            vec![("show".to_string(), vec!["active_users".to_string(), "options".to_string()])];
        app.input = "show ".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "active_users"));
        assert!(app.completions.iter().any(|c| c == "options"));
        assert_eq!(app.completions, vec!["active_users".to_string(), "options".to_string()]);
    }

    #[test]
    fn accept_subcommand_advances_past_separator() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_SUBS.lock().unwrap() =
            vec![("workspace".to_string(), vec!["list".to_string(), "new".to_string()])];
        app.input = "workspace ".into();
        app.refresh_completions();
        let sub = app.completions[0].clone();
        app.accept_completion();
        // 接受子命令后保留分隔空格，光标进入子命令之后的参数位置。
        assert_eq!(app.input, format!("workspace {sub} "));
    }

    #[test]
    fn history_prev_recalls_older_commands() {
        let mut app = App::new();
        app.history = vec!["connect".to_string().into(), "msg hi".to_string().into()];
        app.history_prev();
        assert_eq!(app.input, "msg hi");
        app.history_prev();
        assert_eq!(app.input, "connect");
    }

    #[test]
    fn history_next_returns_to_empty_input() {
        let mut app = App::new();
        app.history = vec!["connect".to_string().into(), "msg hi".to_string().into()];
        app.history_idx = Some(0);
        app.history_next();
        assert_eq!(app.input, "msg hi");
        app.history_next();
        assert!(app.input.is_empty());
        assert_eq!(app.history_idx, None);
    }

    #[test]
    fn completion_highlight_preserved_when_candidates_unchanged() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![("msg".to_string(), 1, 1, String::new())];
        app.members = vec![
            "alice".to_string().into(),
            "andrew".to_string().into(),
            "anna".to_string().into(),
        ];
        app.input = "msg a".into();
        app.refresh_completions();
        assert!(app.completions.len() > 1);
        app.cycle_completion(1);
        let idx = app.comp_idx;
        let scroll = app.comp_scroll;
        // 重绘帧候选未变：高亮与视口保留，不被刷回顶部。
        app.refresh_completions();
        assert_eq!(app.comp_idx, idx);
        assert_eq!(app.comp_scroll, scroll);
    }

    #[test]
    fn completion_view_resets_when_candidates_change() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_ARG_SPEC.lock().unwrap() = vec![("msg".to_string(), 1, 1, String::new())];
        app.members = vec![
            "alice".to_string().into(),
            "andrew".to_string().into(),
            "anna".to_string().into(),
        ];
        app.input = "msg a".into();
        app.refresh_completions();
        app.cycle_completion(1);
        app.completion_active = true;
        // 前缀改变 -> 候选集合变化，回到首项并退出选择态。
        app.input = "msg z".into();
        app.refresh_completions();
        assert_eq!(app.comp_idx, 0);
        assert_eq!(app.comp_scroll, 0);
        assert!(!app.completion_active);
    }

    #[test]
    fn builtin_exit_sets_flag() {
        let mut app = App::new();
        assert!(try_builtin(&mut app, &["exit".to_string()]));
        assert!(app.exit);
        assert!(try_builtin(&mut app, &["q".to_string()]));
        assert!(app.exit);
    }

    #[test]
    fn background_refresh_stays_silent_but_typed_command_echoes() {
        let mut app = App::new();
        let refresh = [
            vec!["show".to_string(), "active_users".to_string()],
            vec!["show".to_string(), "plugins".to_string()],
            vec!["plugin".to_string(), "list".to_string()],
            vec!["sessions".to_string(), "-l".to_string()],
        ];
        // 后台自动投递：静默，不写输出区。
        let before = app.log.len();
        for argv in &refresh {
            run_argv(&mut app, &STUB_RUN, &STUB_DRAIN, argv, true);
        }
        assert_eq!(app.log.len(), before, "background refreshes must stay out of the output log");

        // 用户主动敲入同一条命令：正常回显命令行到输出区。
        for argv in &refresh {
            run_argv(&mut app, &STUB_RUN, &STUB_DRAIN, argv, false);
        }
        assert!(
            app.log.iter().any(|l| l.text.starts_with("tc> plugin list")),
            "a typed refresh command must echo its command line"
        );
    }

    #[test]
    fn builtin_clear_empties_log() {
        let mut app = App::new();
        app.push_line("x".to_string(), LineKind::System);
        assert!(try_builtin(&mut app, &["clear".to_string()]));
        assert!(app.log.is_empty());
    }

    #[test]
    fn builtin_version_opens_info_dialog() {
        let mut app = App::new();
        let before = app.log.len();
        assert!(try_builtin(&mut app, &["version".to_string()]));
        assert!(matches!(app.dialog, Some(Dialog::Info { .. })));
        assert_eq!(app.log.len(), before);
    }

    #[test]
    fn unknown_command_is_not_builtin() {
        let mut app = App::new();
        assert!(!try_builtin(&mut app, &["msg".to_string(), "hi".to_string()]));
    }

    #[test]
    fn builtin_help_opens_modal_not_log() {
        let mut app = App::new();
        let before = app.log.len();
        assert!(try_builtin(&mut app, &["help".to_string()]));
        match &app.dialog {
            Some(Dialog::Info { title, lines }) => {
                assert_eq!(title, "Help");
                assert!(!lines.is_empty());
            }
            _ => panic!("expected Info dialog"),
        }
        assert_eq!(app.log.len(), before);
        assert!(!app.log.iter().any(|l| l.text.contains("Built-in:")));
    }

    #[test]
    fn builtin_license_opens_plain_dialog() {
        let mut app = App::new();
        assert!(try_builtin(&mut app, &["license".to_string()]));
        match &app.dialog {
            Some(Dialog::Info { title, lines }) => {
                assert_eq!(title, "License");
                assert!(!lines.is_empty());
            }
            _ => panic!("expected Info dialog"),
        }
    }

    #[test]
    fn builtin_about_copyright_open_dialogs() {
        let mut app = App::new();
        assert!(try_builtin(&mut app, &["about".to_string()]));
        let title = match &app.dialog {
            Some(Dialog::Info { title, .. }) => title.as_str(),
            _ => "",
        };
        assert_eq!(title, "About");
        assert!(try_builtin(&mut app, &["copyright".to_string()]));
        let title = match &app.dialog {
            Some(Dialog::Info { title, .. }) => title.as_str(),
            _ => "",
        };
        assert_eq!(title, "Copyright");
    }

    #[test]
    fn copyright_text_names_hfunction_author() {
        let text = crate::copyright_text();
        assert!(text.contains("HFunction2013"), "{text}");
        assert!(!text.contains("HZFY"), "{text}");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_nickname_is_hf_two_lowercase_letters() {
        for _ in 0..20 {
            let nick = debug_random_nickname();
            assert!(nick.starts_with("hf"));
            assert_eq!(nick.len(), 4);
            assert!(nick[2..].chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_seed_argv_seeds_defaults_when_empty() {
        let cmds = debug_seed_argv("", "");
        assert_eq!(
            cmds[0],
            vec!["setg".to_string(), "SERVER".to_string(), "127.0.0.1".to_string()]
        );
        assert_eq!(cmds[1], vec!["setg".to_string(), "PORT".to_string(), "50051".to_string()]);
        assert_eq!(cmds[2][0], "setg");
        assert_eq!(cmds[2][1], "USERNAME");
        assert!(cmds[2][2].starts_with("hf") && cmds[2][2].len() == 4);
        assert_eq!(cmds.last().unwrap(), &vec!["connect".to_string()]);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_seed_argv_keeps_explicit_config() {
        let cmds = debug_seed_argv("10.0.0.2", "alice");
        assert!(cmds.iter().all(|c| c[0] != "setg"));
        assert_eq!(cmds, vec![vec!["connect".to_string()]]);
    }

    #[test]
    fn repl_only_commands_are_intercepted_in_tui() {
        for cmd in ["banner", "train", "sleep"] {
            let mut app = App::new();
            assert!(try_builtin(&mut app, &[cmd.to_string()]), "{cmd} should be intercepted");
            assert!(app.log.iter().any(|l| l.text.contains("REPL")), "{cmd} should hint REPL");
        }
        // 编辑器经后台 worker 拉起，前后由终端钩子切换全屏，不在本地拦截。
        let mut app = App::new();
        assert!(!try_builtin(&mut app, &["editor".to_string()]));
    }

    #[test]
    fn truthy_recognizes_on_true_1_yes() {
        for v in ["on", "true", "1", "yes", "ON", "Yes", "True", " y ", "enabled", "enable", "y"] {
            assert!(is_truthy(v), "{v} should be truthy");
        }
        for v in ["", "off", "0", "no", "false", "n", "yep"] {
            assert!(!is_truthy(v), "{v} should be falsy");
        }
    }

    #[test]
    fn form_argv_join_omits_empty_password() {
        let spec = stub_spec("join_freq", "", vec![]);
        let with_pw = form_argv(&spec, &["abc".into(), "secret".into()]);
        assert_eq!(with_pw, vec!["join_freq".to_string(), "abc".to_string(), "secret".to_string()]);

        let no_pw = form_argv(&spec, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["join_freq".to_string(), "abc".to_string()]);
    }

    #[test]
    fn form_argv_create_freq_omits_empty_password() {
        let spec = stub_spec("create_freq", "", vec![]);
        let with_pw = form_argv(&spec, &["abc".into(), "secret".into()]);
        assert_eq!(
            with_pw,
            vec!["create_freq".to_string(), "abc".to_string(), "secret".to_string()]
        );

        let no_pw = form_argv(&spec, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["create_freq".to_string(), "abc".to_string()]);
    }

    #[test]
    fn open_form_initializes_focus_and_values() {
        let mut app = App::new();
        let spec =
            stub_spec("join_freq", "", vec![stub_field(true, false), stub_field(false, true)]);
        app.open_form(&spec);
        let Some(Dialog::Form(form)) = &app.dialog else {
            panic!("expected form dialog");
        };
        assert_eq!(form.focus, 0);
        assert_eq!(form.values.len(), spec.fields.len());
        assert!(app.dialog_buffer().unwrap().is_empty());
    }

    #[test]
    fn form_required_field_blocks_advance() {
        let mut app = App::new();
        let spec =
            stub_spec("join_freq", "", vec![stub_field(true, false), stub_field(false, true)]);
        app.open_form(&spec);
        // 必填的 freq 留空：回车应停留在第一字段。
        dialog_advance(&mut app, &STUB_RUN, &STUB_DRAIN);
        let Dialog::Form(form) = app.dialog.as_ref().unwrap() else {
            panic!("expected form dialog");
        };
        assert_eq!(form.focus, 0);

        // 填入 freq 后推进到可选密码字段。
        app.dialog_buffer().unwrap().push_str("abc");
        dialog_advance(&mut app, &STUB_RUN, &STUB_DRAIN);
        let Dialog::Form(form) = app.dialog.as_ref().unwrap() else {
            panic!("expected form dialog");
        };
        assert_eq!(form.focus, 1);
    }

    #[test]
    fn form_action_prefills_when_prefer_cli() {
        let spec = stub_spec("join_freq", "join_freq ", vec![]);
        // 命令行偏好：退回输入框预填，不弹窗、不执行。
        let before = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.prefer_cli = true;
        app.is_connected = Some(STUB_CONNECTED);
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Form(Box::new(spec.clone())));
        assert_eq!(app.input, spec.prefill.as_str());
        assert!(app.dialog.is_none());
        // 预填路径绝不触发命令执行。
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before);

        // 默认：弹出表单收集参数，同样不在菜单点击时直接执行。
        let before2 = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Form(Box::new(spec.clone())));
        assert!(app.dialog.is_some());
        assert!(app.input.is_empty());
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before2);
    }

    #[test]
    fn registry_form_spec_matches_single_and_compound_commands() {
        *STUB_FORM_CMDS.lock().unwrap() =
            vec!["upload".to_string(), "workspace switch".to_string()];

        let upload = vec!["upload".to_string()];
        assert_eq!(registry_form_spec(&upload, Some(&STUB_FORM_SPEC)).unwrap().cmd, "upload");

        let switch_ws = vec!["workspace".to_string(), "switch".to_string()];
        assert_eq!(
            registry_form_spec(&switch_ws, Some(&STUB_FORM_SPEC)).unwrap().cmd,
            "workspace switch"
        );

        // 只有复合命令的首 token 时不应误匹配。
        let ws_only = vec!["workspace".to_string()];
        assert!(registry_form_spec(&ws_only, Some(&STUB_FORM_SPEC)).is_none());

        let bogus = vec!["definitely_not_a_command".to_string()];
        assert!(registry_form_spec(&bogus, Some(&STUB_FORM_SPEC)).is_none());
    }

    #[test]
    fn missing_arg_command_opens_form_when_not_prefer_cli() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        *STUB_FORM_CMDS.lock().unwrap() = vec!["upload".to_string()];
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        app.get_form_spec = Some(STUB_FORM_SPEC);
        let before = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);

        // 缺参、非命令行偏好：打开 upload 表单，不进 FFI。
        execute_or_dialog(&mut app, &STUB_RUN, &STUB_DRAIN, &["upload".to_string()]);
        let Some(Dialog::Form(form)) = &app.dialog else {
            panic!("expected form dialog");
        };
        assert_eq!(form.spec.cmd.as_str(), "upload");
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before);
    }

    #[test]
    fn missing_arg_command_skips_form_when_prefer_cli() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        app.prefer_cli = true;
        let before = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);

        // 命令行偏好：缺参也不弹窗，直接交命令行/FFI。
        execute_or_dialog(&mut app, &STUB_RUN, &STUB_DRAIN, &["upload".to_string()]);
        assert!(app.dialog.is_none());
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before + 1);
    }

    #[test]
    fn info_command_prints_to_log_when_prefer_cli_and_pops_dialog_otherwise() {
        // 命令行偏好：help/license 等信息命令直接落输出区，不弹 Info 窗。
        let mut app = App::new();
        app.prefer_cli = true;
        assert!(try_builtin(&mut app, &["help".to_string()]));
        assert!(app.dialog.is_none());
        assert!(app.log.iter().any(|l| l.text == "Help"));
        assert!(app.log.iter().any(|l| l.text.contains("Built-in:")));

        // 默认偏好：弹出 Info 窗。
        let mut app = App::new();
        assert!(try_builtin(&mut app, &["help".to_string()]));
        assert!(matches!(app.dialog, Some(Dialog::Info { .. })));
    }

    #[test]
    fn command_with_args_does_not_open_form() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        let before = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);

        execute_or_dialog(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &["upload".to_string(), ".gitignore".to_string()],
        );
        assert!(app.dialog.is_none());
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before + 1);
    }

    #[test]
    fn form_action_blocked_when_not_connected() {
        // 未连接：云端命令不弹表单、不预填，只留提示。
        let mut app = App::new();
        run_action(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &Action::Form(Box::new(stub_spec("join_freq", "", vec![]))),
        );
        assert!(app.dialog.is_none());
        assert!(app.input.is_empty());
    }

    #[test]
    fn form_argv_msg_splits_users_and_flags_message() {
        let argv = form_argv(&stub_spec("msg", "", vec![]), &["alice bob".into(), "hello".into()]);
        assert_eq!(
            argv,
            vec![
                "msg".to_string(),
                "alice".to_string(),
                "bob".to_string(),
                "-m".to_string(),
                "hello".to_string()
            ]
        );
    }

    #[test]
    fn form_argv_msg_omits_empty_users() {
        let argv = form_argv(&stub_spec("msg", "", vec![]), &["".into(), "hi".into()]);
        assert_eq!(argv, vec!["msg".to_string(), "-m".to_string(), "hi".to_string()]);
    }

    #[test]
    fn form_argv_two_arg_commands() {
        assert_eq!(
            form_argv(&stub_spec("mv", "", vec![]), &["a".into(), "b".into()]),
            vec!["mv".to_string(), "a".to_string(), "b".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("cp", "", vec![]), &["x".into(), "y".into()]),
            vec!["cp".to_string(), "x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn form_argv_workspace_switch_and_plugin_load() {
        assert_eq!(
            form_argv(&stub_spec("workspace switch", "", vec![]), &["dev".into()]),
            vec!["workspace".to_string(), "switch".to_string(), "dev".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("load", "", vec![]), &["foo".into()]),
            vec!["load".to_string(), "plugin".to_string(), "foo".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("unload", "", vec![]), &["foo".into()]),
            vec!["unload".to_string(), "plugin".to_string(), "foo".to_string()]
        );
    }

    #[test]
    fn form_argv_shell_splits_command_line() {
        let argv = form_argv(&stub_spec("shell", "", vec![]), &["ls -la".into()]);
        assert_eq!(argv, vec!["shell".to_string(), "ls".to_string(), "-la".to_string()]);
    }

    #[test]
    fn form_argv_schedule_msg_flags_message_and_users() {
        let argv = form_argv(
            &stub_spec("schedule_msg", "", vec![]),
            &["+5m".into(), "hi".into(), "alice bob".into()],
        );
        assert_eq!(
            argv,
            vec![
                "schedule_msg".to_string(),
                "+5m".to_string(),
                "alice".to_string(),
                "bob".to_string(),
                "-m".to_string(),
                "hi".to_string()
            ]
        );
    }

    #[test]
    fn form_argv_namespace_load_unload() {
        assert_eq!(
            form_argv(&stub_spec("load namespace", "", vec![]), &["net".into()]),
            vec!["load".to_string(), "namespace".to_string(), "net".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("unload namespace", "", vec![]), &["net".into()]),
            vec!["unload".to_string(), "namespace".to_string(), "net".to_string()]
        );
    }

    #[test]
    fn form_argv_settings_key_value() {
        assert_eq!(
            form_argv(&stub_spec("set", "", vec![]), &["k".into(), "v".into()]),
            vec!["set".to_string(), "k".to_string(), "v".to_string()]
        );
        // setg 的 value 可选：留空时只带 key。
        assert_eq!(
            form_argv(&stub_spec("setg", "", vec![]), &["k".into(), "".into()]),
            vec!["setg".to_string(), "k".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("get", "", vec![]), &["k".into()]),
            vec!["get".to_string(), "k".to_string()]
        );
    }

    #[test]
    fn form_argv_extra_single_arg_commands() {
        assert_eq!(
            form_argv(&stub_spec("unban", "", vec![]), &["u@h".into()]),
            vec!["unban".to_string(), "u@h".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("op", "", vec![]), &["u@h".into()]),
            vec!["op".to_string(), "u@h".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("mkdir", "", vec![]), &["docs".into()]),
            vec!["mkdir".to_string(), "docs".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("cancel", "", vec![]), &["3".into()]),
            vec!["cancel".to_string(), "3".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("unset", "", vec![]), &["k".into()]),
            vec!["unset".to_string(), "k".to_string()]
        );
        assert_eq!(
            form_argv(&stub_spec("unsetg", "", vec![]), &["k".into()]),
            vec!["unsetg".to_string(), "k".to_string()]
        );
        // 可选频点留空：只发命令本身。
        assert_eq!(
            form_argv(&stub_spec("freq_info", "", vec![]), &["".into()]),
            vec!["freq_info".to_string()]
        );
    }

    #[test]
    fn split_path_prefix_splits_parent_and_base() {
        assert_eq!(split_path_prefix("docs/rep"), ("docs/", "rep"));
        assert_eq!(split_path_prefix("rep"), ("", "rep"));
        assert_eq!(split_path_prefix("/abs/p"), ("/abs/", "p"));
    }

    #[test]
    fn split_host_port_separates_numeric_port() {
        assert_eq!(
            split_host_port("127.0.0.1:50051"),
            ("127.0.0.1".to_string(), Some("50051".to_string()))
        );
        assert_eq!(
            split_host_port("  example.com:8080  "),
            ("example.com".to_string(), Some("8080".to_string()))
        );
        // 末段非数字：整串视为 host，不拆端口。
        assert_eq!(split_host_port("example.com:http"), ("example.com:http".to_string(), None));
        assert_eq!(split_host_port("127.0.0.1"), ("127.0.0.1".to_string(), None));
    }

    #[test]
    fn needs_connection_classifies_cloud_and_local() {
        // 云端 RPC 命令。
        for cmd in [
            "join_freq",
            "create_freq",
            "msg",
            "reply",
            "call",
            "ls",
            "upload",
            "download",
            "kick",
            "ban",
            "op",
            "freq_info",
            "status",
            "whoami",
            "route",
            "foreground",
            "away",
            "notifications",
            "pwd",
        ] {
            assert!(needs_connection(cmd), "{cmd} should require connection");
        }
        // 本地命令（含复合命令首词）。
        for cmd in [
            "workspace switch",
            "workspace new",
            "load",
            "unload",
            "load namespace",
            "unload namespace",
            "shell",
            "setg",
            "getg",
            "set",
            "get",
            "unset",
            "unsetg",
            "lcd",
            "lls",
            "lpwd",
        ] {
            assert!(!needs_connection(cmd), "{cmd} should be local");
        }
    }

    #[test]
    fn builtin_groups_hold_only_exit_and_info_dialogs() {
        let groups = builtin_groups();
        let find = |name: &str| groups.iter().find(|g| g.name == name).expect(name);
        let labels = |g: &MenuGroup| g.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>();

        // File: only the local exit command; business groups are registry-driven.
        assert_eq!(labels(find("File")), vec!["Exit".to_string()]);

        // Help group: About / Copyright / License / Help info dialogs.
        let help = labels(find("Help"));
        assert_eq!(
            help,
            vec![
                "About".to_string(),
                "Copyright".to_string(),
                "License".to_string(),
                "Help".to_string()
            ]
        );

        // Built-in accelerators must be unique.
        let mut accels: Vec<char> = groups.iter().map(|g| g.accel).collect();
        accels.sort();
        assert!(!accels.windows(2).any(|w| w[0] == w[1]), "duplicate accel");
    }

    #[test]
    fn command_arg_kind_maps_path_commands() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        stub_command_table(&mut app);
        // kind: MEMBER=1, REMOTE_PATH=2, LOCAL_PATH=3, NO_COMPLETION=8。
        *STUB_ARG_SPEC.lock().unwrap() = vec![
            ("ls".to_string(), 1, 2, String::new()),
            ("rm".to_string(), 1, 2, String::new()),
            ("mv".to_string(), 2, 2, String::new()),
            ("cp".to_string(), 1, 2, String::new()),
            ("checksum".to_string(), 1, 2, String::new()),
            ("upload".to_string(), 1, 3, String::new()),
            ("upload".to_string(), 2, 2, String::new()),
            ("download".to_string(), 1, 2, String::new()),
            ("download".to_string(), 2, 3, String::new()),
            ("lcd".to_string(), 1, 3, String::new()),
            ("msg".to_string(), 1, 1, String::new()),
            ("msg".to_string(), 2, 8, String::new()),
            ("kick".to_string(), 1, 1, String::new()),
            ("away".to_string(), 1, 8, String::new()),
            ("reply".to_string(), 1, 8, String::new()),
            ("status".to_string(), 1, 8, String::new()),
        ];
        let mut kind_of = |input: &str| {
            app.input = input.into();
            app.completion_context().kind
        };
        assert_eq!(kind_of("ls x"), FieldKind::RemotePath);
        assert_eq!(kind_of("rm x"), FieldKind::RemotePath);
        assert_eq!(kind_of("mv x y"), FieldKind::RemotePath);
        assert_eq!(kind_of("cp x"), FieldKind::RemotePath);
        assert_eq!(kind_of("checksum x"), FieldKind::RemotePath);
        assert_eq!(kind_of("upload l"), FieldKind::LocalPath);
        assert_eq!(kind_of("upload l r"), FieldKind::RemotePath);
        assert_eq!(kind_of("download r"), FieldKind::RemotePath);
        assert_eq!(kind_of("download r l"), FieldKind::LocalPath);
        assert_eq!(kind_of("lcd d"), FieldKind::LocalPath);
        assert_eq!(kind_of("msg a"), FieldKind::Member);
        assert_eq!(kind_of("msg a b"), FieldKind::NoCompletion);
        assert_eq!(kind_of("kick u"), FieldKind::Member);
        // 状态词 / 自由文本 / 无参命令不得补全成员名。
        assert_eq!(kind_of("away s"), FieldKind::NoCompletion);
        assert_eq!(kind_of("reply m"), FieldKind::NoCompletion);
        assert_eq!(kind_of("status x"), FieldKind::NoCompletion);
    }

    #[test]
    fn local_candidates_filters_and_marks_dirs() {
        let dir = std::env::temp_dir().join(format!("tc_comp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("alpha.txt"), b"").unwrap();
        std::fs::write(dir.join("beta.md"), b"").unwrap();
        let parent = dir.to_string_lossy().to_string() + "/";
        assert_eq!(local_candidates(&parent, "al"), vec![format!("{parent}alpha.txt")]);
        assert_eq!(local_candidates(&parent, "su"), vec![format!("{parent}sub/")]);
        // 非根目录候选应带上上级目录入口；前缀不匹配 `..` 时被过滤。
        let all = local_candidates(&parent, "");
        assert!(all.contains(&format!("{parent}../")));
        assert!(!local_candidates(&parent, "al").iter().any(|c| c.ends_with("../")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn has_cmd_line(app: &App, text: &str) -> bool {
        app.log.iter().any(|l| l.kind == LineKind::Cmd && l.text == format!("tc> {text}"))
    }

    #[test]
    fn dangerous_classifies_destructive_commands() {
        for cmd in ["disband", "kick", "ban", "rm"] {
            assert!(is_dangerous(cmd), "{cmd} should be dangerous");
        }
        for cmd in ["msg", "unban", "unignore", "deop", "trop", "leave_freq", "join_freq", "help"] {
            assert!(!is_dangerous(cmd), "{cmd} should not be dangerous");
        }
    }

    #[test]
    fn confirm_message_mentions_target() {
        assert!(confirm_message("disband", &["disband".into()]).contains("cannot be undone"));
        assert_eq!(confirm_message("kick", &["kick".into(), "alice".into()]), "Really kick alice?");
        assert_eq!(confirm_message("ban", &["ban".into(), "bob".into()]), "Really ban bob?");
        assert_eq!(
            confirm_message("rm", &["rm".into(), "dir/file".into()]),
            "Really remove remote file dir/file?"
        );
    }

    #[test]
    fn dangerous_command_opens_confirm_without_running() {
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        app.input = "kick alice".into();
        submit(&mut app, &STUB_RUN, &STUB_DRAIN);
        let Some(Dialog::Confirm { message, argv }) = &app.dialog else {
            panic!("expected confirm dialog");
        };
        assert_eq!(argv, &vec!["kick".to_string(), "alice".to_string()]);
        assert!(message.contains("alice"));
        assert!(!has_cmd_line(&app, "kick alice"));
    }

    #[test]
    fn confirm_dialog_runs_original_argv_on_advance() {
        let mut app = App::new();
        app.open_confirm(
            "Really kick alice?".into(),
            vec!["kick".to_string().into(), "alice".to_string().into()],
        );
        dialog_advance(&mut app, &STUB_RUN, &STUB_DRAIN);
        assert!(app.dialog.is_none());
        assert!(has_cmd_line(&app, "kick alice"));
    }

    #[test]
    fn form_dangerous_submit_opens_confirm_not_executed() {
        let mut app = App::new();
        let spec = stub_spec("kick", "", vec![stub_field(true, false)]);
        app.open_form(&spec);
        app.dialog_buffer().unwrap().push_str("alice");
        dialog_advance(&mut app, &STUB_RUN, &STUB_DRAIN);
        let Some(Dialog::Confirm { argv, .. }) = &app.dialog else {
            panic!("expected confirm dialog after kick form");
        };
        assert_eq!(argv, &vec!["kick".to_string(), "alice".to_string()]);
        assert!(!has_cmd_line(&app, "kick alice"));
    }

    #[test]
    fn disband_menu_opens_confirm_when_connected() {
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Run(vec!["disband".to_string()]));
        let Some(Dialog::Confirm { argv, .. }) = &app.dialog else {
            panic!("expected confirm dialog for disband");
        };
        assert_eq!(argv, &vec!["disband".to_string()]);
        assert!(!has_cmd_line(&app, "disband"));
    }

    #[test]
    fn effective_option_session_overrides_global() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(Some("on"));
        set_global(Some("off"));
        assert_eq!(
            opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "PREFER_COMMAND_LINE"),
            "on"
        );
        set_session(None);
        set_global(None);
    }

    #[test]
    fn effective_option_falls_back_to_global() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(Some("yes"));
        assert_eq!(opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "QUIET"), "yes");
        set_global(None);
    }

    #[test]
    fn effective_option_empty_when_unset() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        assert_eq!(opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "X"), "");
        // 无 session 回调时直接读全局。
        set_global(Some("1"));
        assert_eq!(opt_get_effective(None, &STUB_GLOBAL, "X"), "1");
        set_global(None);
    }

    #[test]
    fn effective_option_empty_session_falls_back_to_global() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        // 会话存了空串不能盖掉全局真值，否则偏好被错误判为关闭。
        set_session(Some(""));
        set_global(Some("on"));
        assert_eq!(
            opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "PREFER_COMMAND_LINE"),
            "on"
        );
        set_session(None);
        set_global(None);
    }

    #[test]
    fn prefer_cli_truthy_from_effective_value() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(Some("true"));
        set_global(None);
        let value = opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "PREFER_COMMAND_LINE");
        assert!(is_truthy(&value));
        set_session(None);
    }

    /// Debug harness: drive a real frame through the framework kernel with a synthetic
    /// resize event (no TTY, no real mouse/keyboard), then read the committed tree back.
    #[test]
    fn tui_frame_snapshot_reads_sidebar_without_tty() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.side_sections = vec![ui_registry::RegSideSection {
            id: "freq".to_string(),
            title: "Session".to_string(),
            default_open: true,
            source: "sessions".to_string(),
        }];
        app.side_items = vec![ui_registry::RegSideItem {
            section: "freq".to_string(),
            prefix: "  ".to_string(),
            label: "{name}".to_string(),
            highlight: false,
            line: 0,
        }];

        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        let freq = tui.testing_find("freq");
        assert!(
            freq.iter().any(|node| node.text.contains("(not joined)")),
            "sidebar shows the unjoined placeholder: {freq:?}"
        );
        let header = tui.testing_find("sidebar-reg-section");
        assert!(!header.is_empty(), "registry sidebar header rendered");
        assert!(
            header[0].text.contains("Session"),
            "section title from registry: {:?}",
            header[0].text
        );
        assert!(!tui.testing_nodes().is_empty());
    }

    /// Clicking a sidebar section header toggles it: the glyph switches
    /// between `#` (expanded) and `>` (collapsed), and the section list is
    /// hidden when collapsed. The sidebar has one shared scrollarea.
    #[cfg(debug_assertions)]
    #[test]
    fn sidebar_section_header_collapses_and_expands_on_click() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.sessions = vec![SessionEntry { name: "1.0".to_string(), current: true }];
        app.members = vec!["alice".to_string(), "bob".to_string()];
        app.side_sections = vec![
            ui_registry::RegSideSection {
                id: "freq".to_string(),
                title: "Session".to_string(),
                default_open: true,
                source: "sessions".to_string(),
            },
            ui_registry::RegSideSection {
                id: "members".to_string(),
                title: "Members".to_string(),
                default_open: true,
                source: "members".to_string(),
            },
        ];
        app.side_items = vec![
            ui_registry::RegSideItem {
                section: "freq".to_string(),
                prefix: "* ".to_string(),
                label: "{name}".to_string(),
                highlight: true,
                line: 0,
            },
            ui_registry::RegSideItem {
                section: "freq".to_string(),
                prefix: "  ".to_string(),
                label: "{name}".to_string(),
                highlight: false,
                line: 0,
            },
            ui_registry::RegSideItem {
                section: "members".to_string(),
                prefix: " • ".to_string(),
                label: "{name}".to_string(),
                highlight: false,
                line: 0,
            },
        ];

        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        // One shared scrollarea wraps the whole sidebar.
        assert!(!tui.testing_find("sidebar-scroll").is_empty());

        let click_at =
            |tui: &mut Tui, app: &mut App, center: Point, state: input::InputMouseState| {
                let mouse = input::InputMouse {
                    state,
                    modifiers: kbmod::NONE,
                    position: center,
                    scroll: Point { x: 0, y: 0 },
                    drag: false,
                };
                draw(tui, Some(Input::Mouse(mouse)), app, &STUB_RUN, &STUB_DRAIN, &groups);
                while tui.needs_settling() {
                    draw(tui, None, app, &STUB_RUN, &STUB_DRAIN, &groups);
                }
            };
        let row_center = |node: &edit_tui::tui::NodeSnapshot| Point {
            x: node.x + node.width / 2,
            y: node.y + node.height / 2,
        };
        let header_of = |tui: &Tui, title: &str| -> edit_tui::tui::NodeSnapshot {
            tui.testing_find("sidebar-reg-section")
                .into_iter()
                .find(|n| n.text.contains(title))
                .expect(title)
        };

        // Default: both sections expanded, glyph `#`, lists rendered.
        let sec_freq = header_of(&tui, "Session");
        assert!(sec_freq.text.contains('#'), "session header expanded: {:?}", sec_freq.text);
        assert!(
            sec_freq.width >= 20,
            "header row stretches across the sidebar: width {}",
            sec_freq.width
        );
        assert!(!tui.testing_find("sidebar-sessions").is_empty(), "session list shown");
        assert!(!tui.testing_find("sidebar-members").is_empty(), "member list shown");

        // Click the Session header row to collapse it.
        let center = row_center(&sec_freq);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_freq = header_of(&tui, "Session");
        assert!(sec_freq.text.contains('>'), "session header collapsed: {:?}", sec_freq.text);
        assert!(
            tui.testing_find("sidebar-sessions").is_empty(),
            "session list hidden after collapse"
        );
        // Members section is untouched and still expanded.
        assert!(!tui.testing_find("sidebar-members").is_empty(), "member list still shown");

        // Click again to re-expand.
        let center = row_center(&sec_freq);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_freq = header_of(&tui, "Session");
        assert!(sec_freq.text.contains('#'), "session header re-expanded: {:?}", sec_freq.text);
        assert!(!tui.testing_find("sidebar-sessions").is_empty(), "session list restored");

        // The Members header toggles independently.
        let sec_members = header_of(&tui, "Members");
        let center = row_center(&sec_members);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_members = header_of(&tui, "Members");
        assert!(sec_members.text.contains('>'), "members header collapsed: {:?}", sec_members.text);
        assert!(
            tui.testing_find("sidebar-members").is_empty(),
            "member list hidden after collapse"
        );
        assert!(!tui.testing_find("sidebar-sessions").is_empty(), "session list still shown");
    }

    /// Registry-driven sidebar: registered section shells render in registration
    /// order and dispatch to the host snapshot by `source`; clicking the header
    /// toggles that section's independent open state.
    #[cfg(debug_assertions)]
    #[test]
    fn registry_sidebar_renders_sections_and_dispatches_snapshot() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.side_sections = vec![ui_registry::RegSideSection {
            id: "members".to_string(),
            title: "Members".to_string(),
            default_open: true,
            source: "members".to_string(),
        }];
        app.side_items = vec![ui_registry::RegSideItem {
            section: "members".to_string(),
            prefix: " • ".to_string(),
            label: "{name}".to_string(),
            highlight: false,
            line: 0,
        }];
        app.members = vec!["alice".to_string()];

        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        let header = tui.testing_find("sidebar-reg-section");
        assert!(!header.is_empty(), "registry section header rendered");
        assert!(
            header[0].text.contains("Members"),
            "section title from registry: {:?}",
            header[0].text
        );
        let members = tui.testing_find("sidebar-members");
        assert!(!members.is_empty(), "members snapshot dispatched to registry section");
        let nodes = tui.testing_nodes();
        assert!(
            nodes.iter().any(|node| node.text.contains("alice")),
            "member name backfilled from snapshot: {members:?}"
        );

        // Legacy hardcoded sections are not rendered while the registry is non-empty.
        assert!(
            tui.testing_find("sec-freq").is_empty(),
            "legacy session section suppressed by registry path"
        );

        // Clicking the registry header collapses its own section.
        let center =
            Point { x: header[0].x + header[0].width / 2, y: header[0].y + header[0].height / 2 };
        let mouse = |state: input::InputMouseState| input::InputMouse {
            state,
            modifiers: kbmod::NONE,
            position: center,
            scroll: Point { x: 0, y: 0 },
            drag: false,
        };
        draw(
            &mut tui,
            Some(Input::Mouse(mouse(input::InputMouseState::Left))),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        draw(
            &mut tui,
            Some(Input::Mouse(mouse(input::InputMouseState::None))),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(
            tui.testing_find("sidebar-members").is_empty(),
            "registry section snapshot hidden after collapse"
        );
    }

    /// The Info dialog's Close button owns the focus, sizes to its label (no full-row
    /// reverse highlight), and is centered under the body text.
    #[cfg(debug_assertions)]
    #[test]
    fn info_dialog_button_is_centered_and_focused() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.open_info("Info".into(), vec!["hello world".to_string().into()]);
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        let buttons = tui.testing_find("info-close");
        let button = buttons.iter().find(|n| n.text == "[Close]").expect("button label leaf");
        assert!(button.focused, "button leaf owns focus, not the parent row");
        assert_eq!(
            button.width,
            "[Close]".chars().count() as CoordType,
            "highlight covers only the label width"
        );
        let contents: Vec<_> = tui.testing_find("content");
        let content = contents.last().expect("dialog content");
        let expected_x = content.x + (content.width - button.width) / 2;
        assert_eq!(button.x, expected_x, "button centered horizontally");
    }

    /// Pressing Enter on the focused Info Close button must close the dialog.
    #[cfg(debug_assertions)]
    #[test]
    fn enter_on_info_button_closes_dialog() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.open_info("Info".into(), vec!["hello world".to_string().into()]);
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_some(), "dialog rendered");

        draw(
            &mut tui,
            Some(Input::Keyboard(vk::RETURN)),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_none(), "Enter activated the Close button");
    }

    /// Open a confirm dialog, pump Esc straight through the kernel, then read the
    /// node tree back to assert the dialog is gone — no TTY, no real keyboard.
    #[test]
    fn esc_through_kernel_closes_confirm_dialog() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.open_confirm(
            "Really kick alice?".into(),
            vec!["kick".to_string().into(), "alice".to_string().into()],
        );

        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(!tui.testing_find("confirm-dialog").is_empty(), "dialog rendered");

        draw(
            &mut tui,
            Some(Input::Keyboard(vk::ESCAPE)),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_none(), "Esc closed the dialog");
        assert!(tui.testing_find("confirm-dialog").is_empty(), "dialog node gone from the tree");
    }

    /// Type printable text through the kernel and read it back from the input buffer.
    #[test]
    fn typed_text_lands_in_input_buffer() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();

        // First frame establishes the tree (and focus) so the editline has a
        // previous-node cache; input is only dispatched from the second frame on.
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        draw(&mut tui, Some(Input::Text("hi")), &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert_eq!(app.input, "hi");
    }

    #[test]
    fn command_dispatched_to_worker_runs_off_ui_thread() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut app = App::new();
        app.worker = Some(CommandWorker::spawn(STUB_RUN));
        let worker = app.worker.clone().unwrap();

        run_argv(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &["sleep".to_string(), "1ms".to_string()],
            false,
        );
        // 命令回显行先落到聊天记录（UI 线程不等待执行完成）。
        assert!(app.log.iter().any(|l| l.text.starts_with("tc> sleep")));

        // 轮询 worker 结果槽：命令在后台线程执行完毕后结果回流。结果槽被写入
        // 即证明 worker 线程确实执行了命令（闭包在 FFI 返回后才写槽）。
        let mut done = None;
        for _ in 0..50 {
            done = worker.take_result();
            if done.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(done.is_some(), "worker result never arrived");
        assert!(!worker.is_running());
    }

    #[test]
    fn silent_background_job_keeps_marker_after_completion() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);

        let mut app = App::new();
        app.worker = Some(CommandWorker::spawn(STUB_RUN));
        let worker = app.worker.clone().unwrap();

        // 后台回填作业按静默投递；静默标记跟随该作业写入 worker 原子。
        run_argv(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &["plugin".to_string(), "list".to_string()],
            true,
        );
        assert!(worker.is_silent(), "background refresh must be marked silent");

        // 等待作业完成；完成回调不得把静默标记复位为非静默，残留输出仍属该静默作业。
        let mut done = None;
        for _ in 0..50 {
            done = worker.take_result();
            if done.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(done.is_some(), "worker result never arrived");
        assert!(worker.is_silent(), "silent marker must survive job completion");

        // 残留行回流：静默作业的输出只进回填缓冲，不进输出区日志。
        app.drain_background("  cli-standard (command: std)", worker.is_silent());
        assert!(
            !app.log.iter().any(|l| l.text.contains("cli-standard")),
            "background refresh output must not echo to the output log"
        );
        assert!(app.worker_output.contains("cli-standard"));

        // 下一条主动命令投递后，静默标记被覆盖为非静默。
        run_argv(&mut app, &STUB_RUN, &STUB_DRAIN, &["connect".to_string()], false);
        assert!(!worker.is_silent(), "typed command must reset non-silent");
    }

    #[test]
    fn drain_background_honors_silent_flag() {
        let mut app = App::new();
        let before = app.log.len();

        // 静默回填：只累积回填文本，不写日志。
        app.drain_background("  alice  AliceA", true);
        assert_eq!(app.log.len(), before);
        assert!(app.worker_output.contains("alice"));

        // 非静默：正常回显到输出区。
        app.drain_background("visible result", false);
        assert!(app.log.iter().any(|l| l.text == "visible result"));
    }

    #[test]
    fn job_result_applies_member_side_effects() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.worker_output = "  alice   AliceA  @+\n  bob     BobB  away\n".to_string();
        let job = JobResult {
            code: 0,
            message: String::new(),
            argv: vec!["active_users".to_string()],
            silent: false,
        };
        apply_job_result(&mut app, job);
        assert_eq!(app.members, vec!["AliceA".to_string(), "BobB".to_string()]);
        assert!(app.worker_output.is_empty());
    }

    #[test]
    fn join_success_schedules_roster_refresh() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.username = "alice".to_string();
        let job = JobResult {
            code: 0,
            message: String::new(),
            argv: vec!["join_freq".to_string(), "1.0".to_string()],
            silent: false,
        };
        apply_job_result(&mut app, job);
        assert!(app.pending_roster, "successful join schedules a roster fetch");
        assert!(app.members.iter().any(|m| m == "alice"));
    }

    #[test]
    fn join_failure_does_not_schedule_roster_refresh() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        let job = JobResult {
            code: 1,
            message: "join failed".into(),
            argv: vec!["join_freq".to_string(), "1.0".to_string()],
            silent: false,
        };
        apply_job_result(&mut app, job);
        assert!(!app.pending_roster);
    }

    #[test]
    fn session_switch_success_clears_old_members_and_refetches() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.members = vec!["old-a".to_string().into(), "old-b".to_string().into()];
        apply_member_side_effects(
            &mut app,
            &["sessions".into(), "-i".into(), "1.0".into()],
            "",
            true,
        );
        assert!(app.members.is_empty(), "switch drops the previous roster");
        assert!(app.pending_roster, "switch refetches the target roster");
    }

    #[test]
    fn session_switch_failure_keeps_members() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        app.members = vec!["old-a".to_string().into()];
        apply_member_side_effects(
            &mut app,
            &["sessions".into(), "--interact".into(), "1.0".into()],
            "",
            false,
        );
        assert_eq!(app.members, vec!["old-a".to_string()]);
        assert!(!app.pending_roster);
    }

    #[test]
    fn moderation_success_schedules_roster_refresh() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        for cmd in ["kick", "ban", "unban", "op", "deop", "trop"] {
            let mut app = App::new();
            apply_member_side_effects(&mut app, &[cmd.to_string(), "alice".to_string()], "", true);
            assert!(app.pending_roster, "{cmd} should refetch the roster");
        }
    }

    #[test]
    fn mute_success_does_not_schedule_roster_refresh() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        let mut app = App::new();
        apply_member_side_effects(&mut app, &["mute".into(), "alice".into()], "", true);
        assert!(!app.pending_roster);
    }

    #[test]
    fn plugin_list_backfills_sidebar_plugins() {
        let mut app = App::new();
        apply_member_side_effects(
            &mut app,
            &["plugin".into(), "list".into()],
            "  cli-standard (command: std)",
            true,
        );
        assert_eq!(app.plugins.len(), 1);
        assert_eq!(app.plugins[0].name, "cli-standard");
        assert_eq!(app.plugins[0].namespace, "std");
    }

    #[test]
    fn show_plugins_backfills_sidebar_plugins() {
        let mut app = App::new();
        apply_member_side_effects(
            &mut app,
            &["show".into(), "plugins".into()],
            "  cli-standard (command: std)",
            true,
        );
        assert_eq!(app.plugins.len(), 1);
    }

    #[test]
    fn plugin_mutation_success_schedules_plugin_refresh() {
        for cmd in ["plugin", "namespace", "load", "unload"] {
            let mut app = App::new();
            apply_member_side_effects(&mut app, &[cmd.to_string(), "x".into()], "", true);
            assert!(app.pending_plugins, "{cmd} should refetch the plugin list");
            assert!(app.pending_ui, "{cmd} should re-read the menu/sidebar registry");
        }
    }

    #[test]
    fn plugin_mutation_failure_does_not_schedule_refresh() {
        let mut app = App::new();
        app.pending_plugins = false;
        apply_member_side_effects(
            &mut app,
            &["plugin".into(), "load".into(), "x".into()],
            "",
            false,
        );
        assert!(!app.pending_plugins);
        assert!(!app.pending_ui);
    }

    #[test]
    fn ctrlc_without_running_command_does_not_interrupt_or_quit() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        INTERRUPT_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.set_interrupted = Some(STUB_SET_INT);

        // 无后台命令在跑：Ctrl+C 不置中断标志，也不退出（退出仍走 Ctrl+Q）。
        assert!(!app.interrupt_command());
        assert!(!app.exit);
        assert_eq!(INTERRUPT_CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(app.log.iter().all(|l| !l.text.contains("Interrupting")));
    }

    #[test]
    fn fire_interrupt_signals_core_and_notifies() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        INTERRUPT_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.set_interrupted = Some(STUB_SET_INT);

        app.fire_interrupt();
        assert_eq!(INTERRUPT_CALLS.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(app.log.iter().any(|l| l.text.contains("Interrupting current command")));
    }

    /// After a mouse-selected completion, focus must return to the input box
    /// instead of staying on the suggestion item.
    #[cfg(debug_assertions)]
    #[test]
    fn refocus_after_accept_returns_focus_to_input() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.completions = vec!["server".to_string()];
        app.comp_idx = 0;
        app.completion_active = true;
        app.refocus_input = true;
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(!app.refocus_input, "refocus flag consumed");
        let cmd = tui.testing_find("cmd");
        assert!(!cmd.is_empty(), "input box rendered");
        assert!(cmd.iter().any(|n| n.focused), "focus returned to the input box");
    }

    /// A real two-click mouse accept on a suggestion must leave focus on the
    /// input box, not on the suggestion item.
    #[cfg(debug_assertions)]
    #[test]
    fn mouse_click_accept_completion_returns_focus_to_input() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() = vec!["server".to_string()];
        let groups = builtin_groups();
        app.input = "se".to_string();
        app.completions = vec!["server".to_string()];
        app.comp_idx = 0;
        app.completion_active = true;
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        let item = tui.testing_find("item");
        assert!(!item.is_empty(), "suggestion rendered");
        let center = Point { x: item[0].x + item[0].width / 2, y: item[0].y + item[0].height / 2 };
        let mouse = |state: input::InputMouseState| input::InputMouse {
            state,
            modifiers: kbmod::NONE,
            position: center,
            scroll: Point { x: 0, y: 0 },
            drag: false,
        };

        // First click selects the item; second click activates it.
        for _ in 0..2 {
            draw(
                &mut tui,
                Some(Input::Mouse(mouse(input::InputMouseState::Left))),
                &mut app,
                &STUB_RUN,
                &STUB_DRAIN,
                &groups,
            );
            while tui.needs_settling() {
                draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
            }
            draw(
                &mut tui,
                Some(Input::Mouse(mouse(input::InputMouseState::None))),
                &mut app,
                &STUB_RUN,
                &STUB_DRAIN,
                &groups,
            );
            while tui.needs_settling() {
                draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
            }
        }

        let cmd = tui.testing_find("cmd");
        assert!(!cmd.is_empty(), "input box rendered");
        assert!(
            cmd.iter().any(|n| n.focused),
            "focus returned to the input box after mouse accept"
        );
    }

    /// Accepting a completion with Tab must keep focus in the input box.
    #[cfg(debug_assertions)]
    #[test]
    fn tab_accept_completion_keeps_focus_in_input() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() = vec!["sessions".to_string()];
        let groups = builtin_groups();
        app.input = "se".to_string();
        app.completions = vec!["sessions".to_string()];
        app.comp_idx = 0;
        app.completion_active = true;
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        draw(&mut tui, Some(Input::Keyboard(vk::TAB)), &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        assert_eq!(app.input, "sessions ", "Tab accepted the completion");
        let cmd = tui.testing_find("cmd");
        assert!(!cmd.is_empty(), "input box rendered");
        assert!(cmd.iter().any(|n| n.focused), "focus stays in the input box");
    }

    /// After the mouse selects a suggestion (focus moves to the item),
    /// pressing Enter to accept it must return focus to the input box.
    #[cfg(debug_assertions)]
    #[test]
    fn enter_on_selected_suggestion_returns_focus_to_input() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 100, height: 30 });
        let mut app = App::new();
        stub_command_table(&mut app);
        *STUB_COMMANDS.lock().unwrap() = vec!["sessions".to_string()];
        let groups = builtin_groups();
        app.input = "se".to_string();
        app.completions = vec!["sessions".to_string()];
        app.comp_idx = 0;
        app.completion_active = true;
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 100, height: 30 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        let item = tui.testing_find("item");
        assert!(!item.is_empty(), "suggestion rendered");
        let center = Point { x: item[0].x + item[0].width / 2, y: item[0].y + item[0].height / 2 };
        let mouse = |state: input::InputMouseState| input::InputMouse {
            state,
            modifiers: kbmod::NONE,
            position: center,
            scroll: Point { x: 0, y: 0 },
            drag: false,
        };
        // Click once to select the item and move focus onto it.
        draw(
            &mut tui,
            Some(Input::Mouse(mouse(input::InputMouseState::Left))),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        draw(
            &mut tui,
            Some(Input::Mouse(mouse(input::InputMouseState::None))),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        // Enter activates the focused suggestion.
        draw(
            &mut tui,
            Some(Input::Keyboard(vk::RETURN)),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }

        assert_eq!(app.input, "sessions ", "Enter accepted the completion");
        let cmd = tui.testing_find("cmd");
        assert!(!cmd.is_empty(), "input box rendered");
        assert!(
            cmd.iter().any(|n| n.focused),
            "focus returned to the input box after Enter accept"
        );
    }

    /// Pressing Enter on the focused Confirm button must confirm the dialog.
    #[cfg(debug_assertions)]
    #[test]
    fn enter_on_confirm_button_confirms_dialog() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.open_confirm(
            "Really kick alice?".into(),
            vec!["kick".to_string().into(), "alice".to_string().into()],
        );
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_some(), "confirm dialog rendered");
        let btn = tui.testing_find("confirm-ok");
        assert!(btn.iter().any(|n| n.focused), "confirm button leaf owns focus");

        draw(
            &mut tui,
            Some(Input::Keyboard(vk::RETURN)),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_none(), "Enter activated the Confirm button");
    }

    /// Clicking the Confirm button with the mouse must confirm the dialog.
    #[cfg(debug_assertions)]
    #[test]
    fn mouse_click_on_confirm_button_confirms_dialog() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(None);
        set_global(None);
        let mut tui = Tui::new().unwrap();
        tui.set_size(Size { width: 80, height: 24 });
        let mut app = App::new();
        let groups = builtin_groups();
        app.open_confirm(
            "Really kick alice?".into(),
            vec!["kick".to_string().into(), "alice".to_string().into()],
        );
        draw(
            &mut tui,
            Some(Input::Resize(Size { width: 80, height: 24 })),
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &groups,
        );
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        let btn = tui.testing_find("confirm-ok");
        let center = Point { x: btn[0].x + btn[0].width / 2, y: btn[0].y + btn[0].height / 2 };
        let down = input::InputMouse {
            state: input::InputMouseState::Left,
            modifiers: kbmod::NONE,
            position: center,
            scroll: Point { x: 0, y: 0 },
            drag: false,
        };
        draw(&mut tui, Some(Input::Mouse(down)), &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        let up = input::InputMouse {
            state: input::InputMouseState::None,
            modifiers: kbmod::NONE,
            position: center,
            scroll: Point { x: 0, y: 0 },
            drag: false,
        };
        draw(&mut tui, Some(Input::Mouse(up)), &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        while tui.needs_settling() {
            draw(&mut tui, None, &mut app, &STUB_RUN, &STUB_DRAIN, &groups);
        }
        assert!(app.dialog.is_none(), "mouse click activated the Confirm button");
    }
}
