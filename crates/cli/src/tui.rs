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
use cli_core_types::plugin_fn_types::ListRemoteDirFn;
use cli_core_types::{IsConnectedFn, RunCommandFn};
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;
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
    /// 弹出参数表单收集参数；`PREFER_COMMAND_LINE` 为真时退回输入框预填。
    Form(&'static FormSpec),
}

/// 表单中的一个输入字段。
struct FormField {
    /// 输入提示，如 `Frequency ID:`。
    label: &'static str,
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
}

/// 一个待提交命令的表单规格（静态）。
struct FormSpec {
    /// 弹窗标题。
    title: &'static str,
    /// 提交后执行的命令名。
    cmd: &'static str,
    /// 命令行偏好下退回输入框的前缀。
    prefill: &'static str,
    fields: &'static [FormField],
}

/// `join_freq` 表单：freq 必填，password 可选。
const JOIN_FORM: FormSpec = FormSpec {
    title: "Join Frequency",
    cmd: "join_freq",
    prefill: "join_freq ",
    fields: &[
        FormField { label: "Frequency ID:", required: true, secret: false, kind: FieldKind::Plain },
        FormField {
            label: "Password (optional):",
            required: false,
            secret: true,
            kind: FieldKind::Plain,
        },
    ],
};

/// `workspace switch` 表单。
const SWITCH_WS_FORM: FormSpec = FormSpec {
    title: "Switch Workspace",
    cmd: "workspace switch",
    prefill: "workspace switch ",
    fields: &[FormField {
        label: "Workspace name:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// `workspace new` 表单：名称可选（服务端默认 `Workspace-xxx`）。
const NEW_WS_FORM: FormSpec = FormSpec {
    title: "New Workspace",
    cmd: "workspace new",
    prefill: "workspace new ",
    fields: &[FormField {
        label: "Workspace name (optional):",
        required: false,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// `create_freq` 表单：频点名必填，密码可选（掩码回显）；成员数等其余参数仍走命令行。
const CREATE_FREQ_FORM: FormSpec = FormSpec {
    title: "Create Frequency",
    cmd: "create_freq",
    prefill: "create_freq ",
    fields: &[
        FormField { label: "Frequency ID:", required: true, secret: false, kind: FieldKind::Plain },
        FormField {
            label: "Password (optional):",
            required: false,
            secret: true,
            kind: FieldKind::Plain,
        },
    ],
};

/// `msg` 表单：收件人可选（默认全员），消息必填。
const MSG_FORM: FormSpec = FormSpec {
    title: "Send Message",
    cmd: "msg",
    prefill: "msg -m ",
    fields: &[
        FormField {
            label: "Users (optional):",
            required: false,
            secret: false,
            kind: FieldKind::Member,
        },
        FormField { label: "Message:", required: true, secret: false, kind: FieldKind::Plain },
    ],
};

/// `reply` 表单。
const REPLY_FORM: FormSpec = FormSpec {
    title: "Reply",
    cmd: "reply",
    prefill: "reply ",
    fields: &[FormField {
        label: "Message:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// `call` 表单。
const CALL_FORM: FormSpec = FormSpec {
    title: "Call User",
    cmd: "call",
    prefill: "call ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};

/// 单用户管理命令表单（mute/unmute/ignore/kick/ban）。
const MUTE_FORM: FormSpec = FormSpec {
    title: "Mute User",
    cmd: "mute",
    prefill: "mute ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const UNMUTE_FORM: FormSpec = FormSpec {
    title: "Unmute User",
    cmd: "unmute",
    prefill: "unmute ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const IGNORE_FORM: FormSpec = FormSpec {
    title: "Ignore User",
    cmd: "ignore",
    prefill: "ignore ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const KICK_FORM: FormSpec = FormSpec {
    title: "Kick User",
    cmd: "kick",
    prefill: "kick ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const BAN_FORM: FormSpec = FormSpec {
    title: "Ban User",
    cmd: "ban",
    prefill: "ban ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};

/// 文件列表表单：目录可选。
const LS_FORM: FormSpec = FormSpec {
    title: "List Remote Files",
    cmd: "ls",
    prefill: "ls ",
    fields: &[FormField {
        label: "Directory (optional):",
        required: false,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};

/// 上传表单：本地路径必填，远端路径可选。
const UPLOAD_FORM: FormSpec = FormSpec {
    title: "Upload",
    cmd: "upload",
    prefill: "upload ",
    fields: &[
        FormField {
            label: "Local path:",
            required: true,
            secret: false,
            kind: FieldKind::LocalPath,
        },
        FormField {
            label: "Remote path (optional):",
            required: false,
            secret: false,
            kind: FieldKind::RemotePath,
        },
    ],
};

/// 下载表单：远端路径必填，本地路径可选。
const DOWNLOAD_FORM: FormSpec = FormSpec {
    title: "Download",
    cmd: "download",
    prefill: "download ",
    fields: &[
        FormField {
            label: "Remote path:",
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
        FormField {
            label: "Local path (optional):",
            required: false,
            secret: false,
            kind: FieldKind::LocalPath,
        },
    ],
};

/// 删除远端文件表单。
const RM_FORM: FormSpec = FormSpec {
    title: "Remove Remote File",
    cmd: "rm",
    prefill: "rm ",
    fields: &[FormField {
        label: "Remote path:",
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};

/// 移动/重命名远端文件表单。
const MV_FORM: FormSpec = FormSpec {
    title: "Move Remote File",
    cmd: "mv",
    prefill: "mv ",
    fields: &[
        FormField { label: "From:", required: true, secret: false, kind: FieldKind::RemotePath },
        FormField { label: "To:", required: true, secret: false, kind: FieldKind::RemotePath },
    ],
};

/// 复制远端文件表单。
const CP_FORM: FormSpec = FormSpec {
    title: "Copy Remote File",
    cmd: "cp",
    prefill: "cp ",
    fields: &[
        FormField { label: "From:", required: true, secret: false, kind: FieldKind::RemotePath },
        FormField { label: "To:", required: true, secret: false, kind: FieldKind::RemotePath },
    ],
};

/// 加载插件表单。
const LOAD_FORM: FormSpec = FormSpec {
    title: "Load Plugin",
    cmd: "load",
    prefill: "load ",
    fields: &[FormField {
        label: "Plugin:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// 卸载插件表单。
const UNLOAD_FORM: FormSpec = FormSpec {
    title: "Unload Plugin",
    cmd: "unload",
    prefill: "unload ",
    fields: &[FormField {
        label: "Plugin name:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// 本地 shell 命令表单。
const SHELL_FORM: FormSpec = FormSpec {
    title: "Local Shell",
    cmd: "shell",
    prefill: "shell ",
    fields: &[FormField {
        label: "Command:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// 单用户管理命令表单（unban/unignore/op/deop/trop/user_info）。
const UNBAN_FORM: FormSpec = FormSpec {
    title: "Unban User",
    cmd: "unban",
    prefill: "unban ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const UNIGNORE_FORM: FormSpec = FormSpec {
    title: "Unignore User",
    cmd: "unignore",
    prefill: "unignore ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const OP_FORM: FormSpec = FormSpec {
    title: "Grant Moderator",
    cmd: "op",
    prefill: "op ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const DEOP_FORM: FormSpec = FormSpec {
    title: "Revoke Moderator",
    cmd: "deop",
    prefill: "deop ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const TROP_FORM: FormSpec = FormSpec {
    title: "Transfer Creator",
    cmd: "trop",
    prefill: "trop ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};
const USER_INFO_FORM: FormSpec = FormSpec {
    title: "User Info",
    cmd: "user_info",
    prefill: "user_info ",
    fields: &[FormField { label: "User:", required: true, secret: false, kind: FieldKind::Member }],
};

/// 远端文件系统命令表单。
const MKDIR_FORM: FormSpec = FormSpec {
    title: "Make Remote Directory",
    cmd: "mkdir",
    prefill: "mkdir ",
    fields: &[FormField {
        label: "Directory:",
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};
const CD_FORM: FormSpec = FormSpec {
    title: "Change Remote Directory",
    cmd: "cd",
    prefill: "cd ",
    fields: &[FormField {
        label: "Directory:",
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};
const CHECKSUM_FORM: FormSpec = FormSpec {
    title: "File Checksum",
    cmd: "checksum",
    prefill: "checksum ",
    fields: &[FormField {
        label: "File:",
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};
const FILE_INFO_FORM: FormSpec = FormSpec {
    title: "File Info",
    cmd: "file_info",
    prefill: "file_info ",
    fields: &[FormField {
        label: "File:",
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
};

/// 定时消息表单：time 位置、message 走 `-m`、users 可选。
const SCHEDULE_MSG_FORM: FormSpec = FormSpec {
    title: "Schedule Message",
    cmd: "schedule_msg",
    prefill: "schedule_msg ",
    fields: &[
        FormField {
            label: "Time (+30s / HH:MM):",
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField { label: "Message:", required: true, secret: false, kind: FieldKind::Plain },
        FormField {
            label: "Users (optional):",
            required: false,
            secret: false,
            kind: FieldKind::Member,
        },
    ],
};
const CANCEL_FORM: FormSpec = FormSpec {
    title: "Cancel Task",
    cmd: "cancel",
    prefill: "cancel ",
    fields: &[FormField {
        label: "Task ID:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// 加载/卸载命名空间表单。
const LOAD_NS_FORM: FormSpec = FormSpec {
    title: "Load Namespace",
    cmd: "load namespace",
    prefill: "load namespace ",
    fields: &[FormField {
        label: "Namespace:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};
const UNLOAD_NS_FORM: FormSpec = FormSpec {
    title: "Unload Namespace",
    cmd: "unload namespace",
    prefill: "unload namespace ",
    fields: &[FormField {
        label: "Namespace:",
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

/// 会话/全局选项表单。
const SET_FORM: FormSpec = FormSpec {
    title: "Set Session Option",
    cmd: "set",
    prefill: "set ",
    fields: &[
        FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain },
        FormField { label: "Value:", required: true, secret: false, kind: FieldKind::Plain },
    ],
};
const GET_FORM: FormSpec = FormSpec {
    title: "Get Session Option",
    cmd: "get",
    prefill: "get ",
    fields: &[FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain }],
};
const SETG_FORM: FormSpec = FormSpec {
    title: "Set Global Option",
    cmd: "setg",
    prefill: "setg ",
    fields: &[
        FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain },
        FormField {
            label: "Value (optional):",
            required: false,
            secret: false,
            kind: FieldKind::Plain,
        },
    ],
};
const GETG_FORM: FormSpec = FormSpec {
    title: "Get Global Option",
    cmd: "getg",
    prefill: "getg ",
    fields: &[FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain }],
};
const UNSET_FORM: FormSpec = FormSpec {
    title: "Unset Session Option",
    cmd: "unset",
    prefill: "unset ",
    fields: &[FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain }],
};
const UNSETG_FORM: FormSpec = FormSpec {
    title: "Unset Global Option",
    cmd: "unsetg",
    prefill: "unsetg ",
    fields: &[FormField { label: "Key:", required: true, secret: false, kind: FieldKind::Plain }],
};

/// 频点信息表单：频点可选（留空取当前）。
const FREQ_INFO_FORM: FormSpec = FormSpec {
    title: "Frequency Info",
    cmd: "freq_info",
    prefill: "freq_info ",
    fields: &[FormField {
        label: "Frequency (optional):",
        required: false,
        secret: false,
        kind: FieldKind::Plain,
    }],
};

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
    "quit",
    "q",
    "clear",
    "version",
    "build_info",
    "copyright",
    "license",
    "coffee",
    "saying",
    "banner",
    "train",
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
            name: "File",
            accel: 'E',
            items: vec![MenuItem { label: "Exit", action: Action::Run(vec!["exit"]) }],
        },
        MenuGroup {
            name: "Connection",
            accel: 'C',
            items: vec![
                MenuItem { label: "Connect", action: Action::Run(vec!["connect"]) },
                MenuItem { label: "Reconnect", action: Action::Run(vec!["reconnect"]) },
                MenuItem { label: "Disconnect", action: Action::Run(vec!["disconnect"]) },
            ],
        },
        MenuGroup {
            name: "Workspace",
            accel: 'W',
            items: vec![
                MenuItem { label: "List", action: Action::Run(vec!["workspace", "list"]) },
                MenuItem { label: "Switch...", action: Action::Form(&SWITCH_WS_FORM) },
                MenuItem { label: "New...", action: Action::Form(&NEW_WS_FORM) },
                MenuItem { label: "Show", action: Action::Run(vec!["workspace", "show"]) },
            ],
        },
        MenuGroup {
            name: "Frequency",
            accel: 'F',
            items: vec![
                MenuItem { label: "Join...", action: Action::Form(&JOIN_FORM) },
                MenuItem { label: "Create...", action: Action::Form(&CREATE_FREQ_FORM) },
                MenuItem { label: "Leave", action: Action::Run(vec!["leave_freq"]) },
                MenuItem { label: "Info...", action: Action::Form(&FREQ_INFO_FORM) },
                MenuItem { label: "Background", action: Action::Run(vec!["background"]) },
                MenuItem { label: "Sessions", action: Action::Run(vec!["sessions", "list"]) },
                MenuItem { label: "Disband...", action: Action::Run(vec!["disband"]) },
                MenuItem {
                    label: "Active freqs",
                    action: Action::Run(vec!["show", "active_freqs"]),
                },
                MenuItem {
                    label: "Active users",
                    action: Action::Run(vec!["show", "active_users"]),
                },
            ],
        },
        MenuGroup {
            name: "Message",
            accel: 'M',
            items: vec![
                MenuItem { label: "Send...", action: Action::Form(&MSG_FORM) },
                MenuItem { label: "Reply...", action: Action::Form(&REPLY_FORM) },
                MenuItem { label: "Call...", action: Action::Form(&CALL_FORM) },
                MenuItem { label: "Schedule...", action: Action::Form(&SCHEDULE_MSG_FORM) },
                MenuItem { label: "Cancel Task...", action: Action::Form(&CANCEL_FORM) },
            ],
        },
        MenuGroup {
            name: "Moderation",
            accel: 'O',
            items: vec![
                MenuItem { label: "Mute...", action: Action::Form(&MUTE_FORM) },
                MenuItem { label: "Unmute...", action: Action::Form(&UNMUTE_FORM) },
                MenuItem { label: "Ignore...", action: Action::Form(&IGNORE_FORM) },
                MenuItem { label: "Kick...", action: Action::Form(&KICK_FORM) },
                MenuItem { label: "Ban...", action: Action::Form(&BAN_FORM) },
                MenuItem { label: "Unban...", action: Action::Form(&UNBAN_FORM) },
                MenuItem { label: "Unignore...", action: Action::Form(&UNIGNORE_FORM) },
                MenuItem { label: "Op...", action: Action::Form(&OP_FORM) },
                MenuItem { label: "Deop...", action: Action::Form(&DEOP_FORM) },
                MenuItem { label: "Trop...", action: Action::Form(&TROP_FORM) },
                MenuItem { label: "User Info...", action: Action::Form(&USER_INFO_FORM) },
            ],
        },
        MenuGroup {
            name: "Files",
            accel: 'L',
            items: vec![
                MenuItem { label: "List...", action: Action::Form(&LS_FORM) },
                MenuItem { label: "Upload...", action: Action::Form(&UPLOAD_FORM) },
                MenuItem { label: "Download...", action: Action::Form(&DOWNLOAD_FORM) },
                MenuItem { label: "Tree", action: Action::Run(vec!["tree"]) },
                MenuItem { label: "Remove...", action: Action::Form(&RM_FORM) },
                MenuItem { label: "Move...", action: Action::Form(&MV_FORM) },
                MenuItem { label: "Copy...", action: Action::Form(&CP_FORM) },
                MenuItem { label: "Mkdir...", action: Action::Form(&MKDIR_FORM) },
                MenuItem { label: "Cd...", action: Action::Form(&CD_FORM) },
                MenuItem { label: "Checksum...", action: Action::Form(&CHECKSUM_FORM) },
                MenuItem { label: "File Info...", action: Action::Form(&FILE_INFO_FORM) },
            ],
        },
        MenuGroup {
            name: "Plugins",
            accel: 'P',
            items: vec![
                MenuItem { label: "Show", action: Action::Run(vec!["show", "plugins"]) },
                MenuItem { label: "Load...", action: Action::Form(&LOAD_FORM) },
                MenuItem { label: "Unload...", action: Action::Form(&UNLOAD_FORM) },
                MenuItem { label: "Load Namespace...", action: Action::Form(&LOAD_NS_FORM) },
                MenuItem { label: "Unload Namespace...", action: Action::Form(&UNLOAD_NS_FORM) },
                MenuItem { label: "Shell...", action: Action::Form(&SHELL_FORM) },
            ],
        },
        MenuGroup {
            name: "Settings",
            accel: 'S',
            items: vec![
                MenuItem { label: "Set...", action: Action::Form(&SET_FORM) },
                MenuItem { label: "Get...", action: Action::Form(&GET_FORM) },
                MenuItem { label: "Set Global...", action: Action::Form(&SETG_FORM) },
                MenuItem { label: "Get Global...", action: Action::Form(&GETG_FORM) },
                MenuItem { label: "Unset...", action: Action::Form(&UNSET_FORM) },
                MenuItem { label: "Unset Global...", action: Action::Form(&UNSETG_FORM) },
                MenuItem { label: "Options", action: Action::Run(vec!["show", "options"]) },
                MenuItem { label: "Locals", action: Action::Run(vec!["show", "locals"]) },
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

/// 激活中的弹窗。
enum Dialog {
    /// 连接配置：先服务器后昵称，两阶段。
    Connect { stage: DialogStage, server: String, username: String },
    /// 命令参数表单：按字段逐阶段收集。
    Form(FormDialog),
    /// 危险操作二次确认：确认后执行 `argv`，取消则丢弃。
    Confirm { message: String, argv: Vec<String> },
}

/// 通用命令表单弹窗状态。
struct FormDialog {
    spec: &'static FormSpec,
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
    /// 激活中的配置弹窗；`None` 表示无弹窗。
    dialog: Option<Dialog>,
    /// 侧栏快照：当前会话 freq。
    current_freq: Option<String>,
    /// 侧栏快照：收集到的成员名。
    members: Vec<String>,
    /// 远端目录列举回调（cli-core `remote_list_dir`）；缺失时远端路径不补全。
    remote_list_dir: Option<ListRemoteDirFn>,
    /// 连接态查询回调（cli-core `is_connected`）；缺失时视为未连接。
    is_connected: Option<IsConnectedFn>,
    /// 状态栏快照（来自全局选项）。
    username: String,
    server: String,
    port: String,
    quiet: bool,
    /// 优先命令行：`PREFER_COMMAND_LINE` 为真值时，表单动作退回输入框预填。
    prefer_cli: bool,
    /// 退出标志。
    exit: bool,
    /// 菜单栏 / 状态栏主题背景（对齐 upstream edit 配色）。
    menubar_bg: StraightRgba,
    /// 菜单栏 / 状态栏主题前景。
    menubar_fg: StraightRgba,
}

impl App {
    fn new() -> Self {
        let mut log = Vec::new();
        log.push(ChatLine {
            text: "[tui] Chat: F10/Alt open menu, Enter run command, Tab complete, Ctrl+Q quit."
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
            remote_list_dir: None,
            is_connected: None,
            username: String::new(),
            server: String::new(),
            port: String::new(),
            quiet: false,
            prefer_cli: false,
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
        self.dialog = Some(Dialog::Connect {
            stage,
            server: self.server.clone(),
            username: self.username.clone(),
        });
    }

    /// 打开命令参数表单弹窗：按字段逐阶段收集。
    fn open_form(&mut self, spec: &'static FormSpec) {
        self.dialog = Some(Dialog::Form(FormDialog {
            spec,
            values: vec![String::new(); spec.fields.len()],
            focus: 0,
        }));
    }

    /// 打开危险操作二次确认弹窗。
    fn open_confirm(&mut self, message: String, argv: Vec<String>) {
        self.dialog = Some(Dialog::Confirm { message, argv });
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
            Dialog::Confirm { .. } => None,
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

    /// 判定当前活动缓冲的补全类别与前缀。
    fn completion_context(&self) -> (FieldKind, String) {
        if let Some(Dialog::Form(form)) = &self.dialog {
            let kind = form.spec.fields[form.focus].kind;
            return (kind, form.values[form.focus].clone());
        }
        let parts: Vec<&str> = self.input.split_whitespace().collect();
        match parts.as_slice() {
            [] => (FieldKind::Plain, String::new()),
            [first] => (FieldKind::Plain, (*first).to_string()),
            _ => {
                let cmd = parts[0];
                let arg_index = parts.len() - 1; // 命令后第几个参数（1 起）
                let kind = kind_for_command_arg(cmd, arg_index);
                (kind, parts[parts.len() - 1].to_string())
            }
        }
    }

    /// 依据当前类别刷新补全候选：命令名 / 成员名 / 远端路径 / 本地路径。
    fn refresh_completions(&mut self) {
        let (kind, prefix) = self.completion_context();
        let cands: Vec<String> = match kind {
            FieldKind::Plain => {
                // 表单内自由文本不补全；主命令行首词匹配命令名。
                if self.dialog.is_some() {
                    Vec::new()
                } else {
                    COMMAND_NAMES
                        .iter()
                        .copied()
                        .filter(|c| c.starts_with(&prefix))
                        .map(String::from)
                        .collect()
                }
            }
            FieldKind::Member => {
                self.members.iter().filter(|m| m.starts_with(&prefix)).cloned().collect()
            }
            FieldKind::RemotePath => {
                let (parent, base) = split_path_prefix(&prefix);
                self.remote_candidates(parent, base)
            }
            FieldKind::LocalPath => {
                let (parent, base) = split_path_prefix(&prefix);
                local_candidates(parent, base)
            }
        };
        self.completions = cands;
        if self.comp_idx >= self.completions.len() {
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

    /// 接受当前高亮候选：替换活动缓冲的末词（路径候选已含父目录前缀）。
    fn accept_completion(&mut self) {
        let Some(cand) = self.completions.get(self.comp_idx).cloned() else {
            return;
        };
        let Some(buf) = self.active_buffer() else {
            return;
        };
        let mut parts: Vec<&str> = buf.split_whitespace().collect();
        if parts.len() <= 1 {
            *buf = cand;
        } else {
            parts.pop();
            parts.push(cand.as_str());
            *buf = parts.join(" ");
        }
        self.refresh_completions();
    }

    /// 在候选列表中循环切换高亮下标。
    fn cycle_completion(&mut self, delta: isize) {
        let n = self.completions.len();
        if n == 0 {
            return;
        }
        self.comp_idx =
            if delta > 0 { (self.comp_idx + 1) % n } else { (self.comp_idx + n - 1) % n };
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
            "leave_freq" | "disband" => self.current_freq = None,
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

/// 按命令名与参数位置判定主命令行该位置的补全类别。
fn kind_for_command_arg(cmd: &str, arg_index: usize) -> FieldKind {
    match cmd {
        // 单参数远端路径命令。
        "ls" | "cd" | "mkdir" | "rm" | "checksum" | "file_info" => FieldKind::RemotePath,
        // mv / cp：from、to 均为远端路径。
        "mv" | "cp" => FieldKind::RemotePath,
        // upload：第 1 参本地路径，第 2 参远端路径。
        "upload" => {
            if arg_index == 1 {
                FieldKind::LocalPath
            } else {
                FieldKind::RemotePath
            }
        }
        // download：第 1 参远端路径，第 2 参本地路径。
        "download" => {
            if arg_index == 1 {
                FieldKind::RemotePath
            } else {
                FieldKind::LocalPath
            }
        }
        // lcd：切换本地目录。
        "lcd" => FieldKind::LocalPath,
        // 其余参数按成员名补全。
        _ => FieldKind::Member,
    }
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

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

/// 读取有效选项值：会话（session）设置优先，未设置再回退全局（global）。
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
            return v.to_string();
        }
    }
    opt_get(get_global, key)
}

/// 全局选项真值判定：`on/true/1/yes`（大小写不敏感）。
fn is_truthy(v: &str) -> bool {
    matches!(v.to_lowercase().as_str(), "on" | "true" | "1" | "yes")
}

fn is_quiet(v: &str) -> bool {
    is_truthy(v)
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
) -> Result<()> {
    let groups = menu_groups();

    loop {
        // 刷新状态栏快照：会话设置优先，未设置回退全局。
        app.username = opt_get_effective(get_session, get_option, "USERNAME");
        app.server = opt_get_effective(get_session, get_option, "SERVER");
        app.port = opt_get_effective(get_session, get_option, "PORT");
        app.quiet = is_quiet(&opt_get_effective(get_session, get_option, "QUIET"));
        app.prefer_cli =
            is_truthy(&opt_get_effective(get_session, get_option, "PREFER_COMMAND_LINE"));

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

    // 鼠标滚轮：上滚回看历史，下滚回到最新；与 PageUp/PageDown 复用同一套
    // App 滚动状态，消费输入后框架不再按内部偏移二次累加。
    if app.dialog.is_none() {
        let wheel = ctx.scroll_delta();
        if wheel.y < 0 {
            app.scroll_up((-wheel.y) as usize);
            ctx.set_input_consumed();
        } else if wheel.y > 0 {
            app.scroll_down(wheel.y as usize);
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

fn draw_body(ctx: &mut Context, app: &App) {
    // 内部纵向：菜单栏(1) + 主体 + 输入框(1) + 状态栏(1)。
    let body_height = (ctx.size().height - 3).max(1);

    ctx.table_begin("body");
    ctx.table_set_columns(&[26, COORD_TYPE_SAFE_MAX]);
    ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: body_height });
    ctx.table_next_row();

    // 左侧：会话 / 成员。
    ctx.block_begin("sidebar");
    ctx.attr_padding(Rect::two(0, 1));
    ctx.attr_intrinsic_size(Size { width: 26, height: body_height });

    ctx.label("sec-freq", "Session");
    let freq = app.current_freq.clone().unwrap_or_else(|| "(not joined)".to_string());
    ctx.label("freq", &format!("  {freq}"));
    ctx.label("blank", "");

    ctx.label("sec-members", "Members");
    if app.members.is_empty() {
        ctx.label("none", "  (none)");
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

    // 依据当前输入刷新补全候选。
    app.refresh_completions();

    // 按键判断必须放在候选浮层挂载之前：浮层 list 会被表格包成第二个
    // 兄弟行，list_end 后 last_node 停在浮层行，而焦点在含输入框的兄弟行，
    // 此时再判断子树归属会失配导致 Enter 不触发提交。当前节点仍是包含
    // 输入框的行容器，用 contains_focus 判断子树是否聚焦。
    if ctx.contains_focus() {
        if ctx.consume_shortcut(vk::UP) {
            if !app.completions.is_empty() {
                app.cycle_completion(-1);
            } else {
                app.history_prev();
            }
        } else if ctx.consume_shortcut(vk::DOWN) {
            if !app.completions.is_empty() {
                app.cycle_completion(1);
            } else {
                app.history_next();
            }
        } else if ctx.consume_shortcut(vk::TAB) {
            if !app.completions.is_empty() {
                app.accept_completion();
            }
        } else if ctx.consume_shortcut(vk::RETURN) {
            submit(app, run_command, output_drain);
        }
    }

    // 候选非空时叠加补全浮层（主输入框与表单弹窗共用）。
    draw_suggestions(ctx, app);

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
    for (idx, cand) in cands.iter().enumerate() {
        let sel = ctx.list_item(idx == app.comp_idx, cand);
        if sel == ListSelection::Activated {
            app.comp_idx = idx;
            app.accept_completion();
        } else if sel == ListSelection::Selected {
            app.comp_idx = idx;
        }
    }
    ctx.list_end();
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
    let text = format!("{who}@{host} | freq {freq} | {quiet} | {}", now_ts());

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
            (form.spec.title, field.label, field.secret)
        }
        Dialog::Confirm { .. } => unreachable!("confirm dialog rendered above"),
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
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            app.note_command(&argv);
            execute_or_dialog(app, run_command, output_drain, &argv);
        }
        Action::Form(spec) => {
            if needs_connection(spec.cmd) && !app.connected() {
                app.push_line("Not connected. Run `connect` first.".to_string(), LineKind::Result);
            } else if app.prefer_cli {
                // 优先命令行：退回输入框预填前缀，交用户补全参数。
                app.input = spec.prefill.to_string();
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
    // 危险操作：先弹二次确认框，确认后再执行。
    if is_dangerous(cmd) {
        app.open_confirm(confirm_message(cmd, argv), argv.to_vec());
        return;
    }
    run_argv(app, run_command, output_drain, argv);
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
            app.push_line(crate::welcome_text(false), LineKind::Result);
            app.follow_latest();
            true
        }
        "build_info" => {
            app.push_line(crate::build_info_text(), LineKind::Result);
            app.follow_latest();
            true
        }
        "copyright" => {
            app.push_line(crate::copyright_text(), LineKind::Result);
            true
        }
        "license" => {
            app.push_line(crate::license_text(), LineKind::Result);
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
            app.push_line(builtin_help_text(), LineKind::Result);
            app.follow_latest();
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

/// 内置 / 常用命令帮助文本。
fn builtin_help_text() -> String {
    [
        "Built-in: exit/quit/q  quit | clear  clear log | version  version | build_info  build info",
        "          copyright  copyright | license  license | coffee  coffee | saying  fortune | help  this help",
        "Commands: connect  connect | msg -m <text>  send message | join_freq <freq> [password] | create_freq <freq>",
        "          show active_users | show active_freqs | workspace list | ls | upload | download",
    ]
    .join("\n")
}

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
    }
}

/// 确认框通过：执行原待提交 argv。
fn finish_confirm(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(Dialog::Confirm { argv, .. }) = app.dialog.take() else { return };
    app.note_command(&argv);
    run_argv(app, run_command, output_drain, &argv);
}

/// 弹窗完成：把收集到的服务器 / 昵称经 `setg` 回填全局选项，再执行 `connect`。
fn finish_dialog(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(d) = app.dialog.take() else { return };
    let (server, username) = match d {
        Dialog::Connect { server, username, .. } => (server, username),
        Dialog::Form(_) | Dialog::Confirm { .. } => return,
    };
    if !server.is_empty() {
        let (host, port) = split_host_port(&server);
        if !host.is_empty() {
            run_argv(
                app,
                run_command,
                output_drain,
                &["setg".to_string(), "SERVER".to_string(), host],
            );
        }
        if let Some(p) = port {
            run_argv(app, run_command, output_drain, &["setg".to_string(), "PORT".to_string(), p]);
        }
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

/// 按命令名把表单字段值组装成 argv（纯函数，便于单测）。
fn form_argv(spec: &FormSpec, vals: &[String]) -> Vec<String> {
    let trimmed: Vec<String> = vals.iter().map(|s| s.trim().to_string()).collect();
    let opt = |i: usize| trimmed.get(i).filter(|s| !s.is_empty()).cloned();

    match spec.cmd {
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
            let mut argv = vec!["workspace".into(), "switch".into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            argv
        }
        "workspace new" => {
            let mut argv = vec!["workspace".into(), "new".into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            argv
        }
        "create_freq" => {
            let mut argv = vec!["create_freq".into()];
            if let Some(v) = opt(0) {
                argv.push(v);
            }
            if let Some(pw) = opt(1) {
                argv.push(pw);
            }
            argv
        }
        "msg" => {
            let mut argv = vec!["msg".into()];
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
            let mut argv = vec!["reply".into()];
            if let Some(msg) = opt(0) {
                argv.push("-m".into());
                argv.push(msg);
            }
            argv
        }
        "call" | "mute" | "unmute" | "ignore" | "kick" | "ban" | "rm" | "unban" | "unignore"
        | "op" | "deop" | "trop" | "user_info" | "mkdir" | "cd" | "checksum" | "file_info"
        | "cancel" | "get" | "getg" | "unset" | "unsetg" => {
            let mut argv = vec![spec.cmd.to_string()];
            if let Some(a) = opt(0) {
                argv.push(a);
            }
            argv
        }
        "ls" => {
            let mut argv = vec!["ls".into()];
            if let Some(d) = opt(0) {
                argv.push(d);
            }
            argv
        }
        "freq_info" => {
            let mut argv = vec!["freq_info".into()];
            if let Some(f) = opt(0) {
                argv.push(f);
            }
            argv
        }
        "set" => {
            let mut argv = vec!["set".into()];
            if let Some(k) = opt(0) {
                argv.push(k);
            }
            if let Some(v) = opt(1) {
                argv.push(v);
            }
            argv
        }
        "setg" => {
            let mut argv = vec!["setg".into()];
            if let Some(k) = opt(0) {
                argv.push(k);
            }
            if let Some(v) = opt(1) {
                argv.push(v);
            }
            argv
        }
        "schedule_msg" => {
            let mut argv = vec!["schedule_msg".into()];
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
            let mut argv = vec!["upload".into()];
            if let Some(local) = opt(0) {
                argv.push(local);
            }
            if let Some(remote) = opt(1) {
                argv.push(remote);
            }
            argv
        }
        "download" => {
            let mut argv = vec!["download".into()];
            if let Some(remote) = opt(0) {
                argv.push(remote);
            }
            if let Some(local) = opt(1) {
                argv.push(local);
            }
            argv
        }
        "mv" | "cp" => {
            let mut argv = vec![spec.cmd.to_string()];
            if let Some(a) = opt(0) {
                argv.push(a);
            }
            if let Some(b) = opt(1) {
                argv.push(b);
            }
            argv
        }
        "load" => {
            let mut argv = vec!["load".into(), "plugin".into()];
            if let Some(p) = opt(0) {
                argv.push(p);
            }
            argv
        }
        "unload" => {
            let mut argv = vec!["unload".into(), "plugin".into()];
            if let Some(p) = opt(0) {
                argv.push(p);
            }
            argv
        }
        "load namespace" => {
            let mut argv = vec!["load".into(), "namespace".into()];
            if let Some(ns) = opt(0) {
                argv.push(ns);
            }
            argv
        }
        "unload namespace" => {
            let mut argv = vec!["unload".into(), "namespace".into()];
            if let Some(ns) = opt(0) {
                argv.push(ns);
            }
            argv
        }
        "shell" => {
            let mut argv = vec!["shell".into()];
            if let Some(cmd) = opt(0) {
                argv.extend(shell_words::split(&cmd).unwrap_or_else(|_| vec![cmd]));
            }
            argv
        }
        _ => vec![spec.cmd.to_string()],
    }
}

/// 表单完成：按命令名把收集到的字段组装成 argv 并执行。
fn finish_form(app: &mut App, run_command: &RunCommandFn, output_drain: &OutputDrainFn) {
    let Some(Dialog::Form(form)) = app.dialog.take() else { return };
    let vals: Vec<String> = form.values.iter().map(|s| s.trim().to_string()).collect();
    let argv = form_argv(form.spec, &vals);
    let cmd = argv.first().map(|s| s.as_str()).unwrap_or("");
    if is_dangerous(cmd) {
        app.open_confirm(confirm_message(cmd, &argv), argv);
        return;
    }
    app.note_command(&argv);
    run_argv(app, run_command, output_drain, &argv);
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

    extern "C" fn stub_run(_: &safer_ffi::Vec<safer_ffi::String>) -> cli_core_types::Result {
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
    fn accept_completion_fills_first_token() {
        let mut app = App::new();
        app.input = "con".into();
        app.refresh_completions();
        let first = app.completions[0].clone();
        app.accept_completion();
        assert_eq!(app.input, first);
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
    fn builtin_clear_empties_log() {
        let mut app = App::new();
        app.push_line("x".to_string(), LineKind::System);
        assert!(try_builtin(&mut app, &["clear".to_string()]));
        assert!(app.log.is_empty());
    }

    #[test]
    fn builtin_version_appends_text() {
        let mut app = App::new();
        let before = app.log.len();
        assert!(try_builtin(&mut app, &["version".to_string()]));
        assert!(app.log.len() > before);
    }

    #[test]
    fn unknown_command_is_not_builtin() {
        let mut app = App::new();
        assert!(!try_builtin(&mut app, &["msg".to_string(), "hi".to_string()]));
    }

    #[test]
    fn truthy_recognizes_on_true_1_yes() {
        for v in ["on", "true", "1", "yes", "ON", "Yes", "True"] {
            assert!(is_truthy(v), "{v} should be truthy");
        }
        for v in ["", "off", "0", "no", "false", "n", "yep"] {
            assert!(!is_truthy(v), "{v} should be falsy");
        }
    }

    #[test]
    fn form_argv_join_omits_empty_password() {
        let with_pw = form_argv(&JOIN_FORM, &["abc".into(), "secret".into()]);
        assert_eq!(with_pw, vec!["join_freq", "abc", "secret"]);

        let no_pw = form_argv(&JOIN_FORM, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["join_freq", "abc"]);
    }

    #[test]
    fn form_argv_create_freq_omits_empty_password() {
        let with_pw = form_argv(&CREATE_FREQ_FORM, &["abc".into(), "secret".into()]);
        assert_eq!(with_pw, vec!["create_freq", "abc", "secret"]);

        let no_pw = form_argv(&CREATE_FREQ_FORM, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["create_freq", "abc"]);
    }

    #[test]
    fn open_form_initializes_focus_and_values() {
        let mut app = App::new();
        app.open_form(&JOIN_FORM);
        let Some(Dialog::Form(form)) = &app.dialog else {
            panic!("expected form dialog");
        };
        assert_eq!(form.focus, 0);
        assert_eq!(form.values.len(), JOIN_FORM.fields.len());
        assert!(app.dialog_buffer().unwrap().is_empty());
    }

    #[test]
    fn form_required_field_blocks_advance() {
        let mut app = App::new();
        app.open_form(&JOIN_FORM);
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
        // 命令行偏好：退回输入框预填，不弹窗。
        let mut app = App::new();
        app.prefer_cli = true;
        app.is_connected = Some(STUB_CONNECTED);
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Form(&JOIN_FORM));
        assert_eq!(app.input, JOIN_FORM.prefill);
        assert!(app.dialog.is_none());

        // 默认：弹出表单收集参数。
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Form(&JOIN_FORM));
        assert!(app.dialog.is_some());
        assert!(app.input.is_empty());
    }

    #[test]
    fn form_action_blocked_when_not_connected() {
        // 未连接：云端命令不弹表单、不预填，只留提示。
        let mut app = App::new();
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Form(&JOIN_FORM));
        assert!(app.dialog.is_none());
        assert!(app.input.is_empty());
    }

    #[test]
    fn form_argv_msg_splits_users_and_flags_message() {
        let argv = form_argv(&MSG_FORM, &["alice bob".into(), "hello".into()]);
        assert_eq!(argv, vec!["msg", "alice", "bob", "-m", "hello"]);
    }

    #[test]
    fn form_argv_msg_omits_empty_users() {
        let argv = form_argv(&MSG_FORM, &["".into(), "hi".into()]);
        assert_eq!(argv, vec!["msg", "-m", "hi"]);
    }

    #[test]
    fn form_argv_two_arg_commands() {
        assert_eq!(form_argv(&MV_FORM, &["a".into(), "b".into()]), vec!["mv", "a", "b"]);
        assert_eq!(form_argv(&CP_FORM, &["x".into(), "y".into()]), vec!["cp", "x", "y"]);
    }

    #[test]
    fn form_argv_workspace_switch_and_plugin_load() {
        assert_eq!(form_argv(&SWITCH_WS_FORM, &["dev".into()]), vec!["workspace", "switch", "dev"]);
        assert_eq!(form_argv(&LOAD_FORM, &["foo".into()]), vec!["load", "plugin", "foo"]);
        assert_eq!(form_argv(&UNLOAD_FORM, &["foo".into()]), vec!["unload", "plugin", "foo"]);
    }

    #[test]
    fn form_argv_shell_splits_command_line() {
        let argv = form_argv(&SHELL_FORM, &["ls -la".into()]);
        assert_eq!(argv, vec!["shell", "ls", "-la"]);
    }

    #[test]
    fn form_argv_schedule_msg_flags_message_and_users() {
        let argv = form_argv(&SCHEDULE_MSG_FORM, &["+5m".into(), "hi".into(), "alice bob".into()]);
        assert_eq!(argv, vec!["schedule_msg", "+5m", "alice", "bob", "-m", "hi"]);
    }

    #[test]
    fn form_argv_namespace_load_unload() {
        assert_eq!(form_argv(&LOAD_NS_FORM, &["net".into()]), vec!["load", "namespace", "net"]);
        assert_eq!(form_argv(&UNLOAD_NS_FORM, &["net".into()]), vec!["unload", "namespace", "net"]);
    }

    #[test]
    fn form_argv_settings_key_value() {
        assert_eq!(form_argv(&SET_FORM, &["k".into(), "v".into()]), vec!["set", "k", "v"]);
        // setg 的 value 可选：留空时只带 key。
        assert_eq!(form_argv(&SETG_FORM, &["k".into(), "".into()]), vec!["setg", "k"]);
        assert_eq!(form_argv(&GET_FORM, &["k".into()]), vec!["get", "k"]);
    }

    #[test]
    fn form_argv_extra_single_arg_commands() {
        assert_eq!(form_argv(&UNBAN_FORM, &["u@h".into()]), vec!["unban", "u@h"]);
        assert_eq!(form_argv(&OP_FORM, &["u@h".into()]), vec!["op", "u@h"]);
        assert_eq!(form_argv(&MKDIR_FORM, &["docs".into()]), vec!["mkdir", "docs"]);
        assert_eq!(form_argv(&CANCEL_FORM, &["3".into()]), vec!["cancel", "3"]);
        assert_eq!(form_argv(&UNSET_FORM, &["k".into()]), vec!["unset", "k"]);
        assert_eq!(form_argv(&UNSETG_FORM, &["k".into()]), vec!["unsetg", "k"]);
        // 可选频点留空：只发命令本身。
        assert_eq!(form_argv(&FREQ_INFO_FORM, &["".into()]), vec!["freq_info"]);
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
        ] {
            assert!(!needs_connection(cmd), "{cmd} should be local");
        }
    }

    #[test]
    fn menu_groups_place_entries_in_correct_group() {
        let groups = menu_groups();
        let find = |name: &str| groups.iter().find(|g| g.name == name).expect(name);
        let labels = |g: &MenuGroup| g.items.iter().map(|i| i.label).collect::<Vec<_>>();

        // File: Exit only; Connection must not hold Exit.
        assert_eq!(labels(find("File")), vec!["Exit"]);
        assert!(!labels(find("Connection")).contains(&"Exit"));

        // Frequency: disband lifecycle + online queries.
        let freq = labels(find("Frequency"));
        assert!(freq.contains(&"Disband..."));
        assert!(freq.contains(&"Active freqs"));
        assert!(freq.contains(&"Active users"));

        // Moderation: a single Ignore, no Disband.
        let moderation = labels(find("Moderation"));
        assert_eq!(moderation.iter().filter(|l| **l == "Ignore...").count(), 1);
        assert!(!moderation.contains(&"Disband..."));

        // Settings: option inspection; Plugins: local shell.
        let settings = labels(find("Settings"));
        assert!(settings.contains(&"Options"));
        assert!(settings.contains(&"Locals"));
        assert!(labels(find("Plugins")).contains(&"Shell..."));

        // Help group was removed after its entries were re-homed.
        assert!(groups.iter().all(|g| g.name != "Help"));

        // Menu accelerators must be unique.
        let mut accels: Vec<char> = groups.iter().map(|g| g.accel).collect();
        accels.sort();
        assert!(!accels.windows(2).any(|w| w[0] == w[1]), "duplicate accel");
    }

    #[test]
    fn command_arg_kind_maps_path_commands() {
        assert_eq!(kind_for_command_arg("ls", 1), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("rm", 1), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("mv", 2), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("cp", 1), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("checksum", 1), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("upload", 1), FieldKind::LocalPath);
        assert_eq!(kind_for_command_arg("upload", 2), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("download", 1), FieldKind::RemotePath);
        assert_eq!(kind_for_command_arg("download", 2), FieldKind::LocalPath);
        assert_eq!(kind_for_command_arg("lcd", 1), FieldKind::LocalPath);
        assert_eq!(kind_for_command_arg("msg", 1), FieldKind::Member);
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
        app.open_confirm("Really kick alice?".into(), vec!["kick".into(), "alice".into()]);
        dialog_advance(&mut app, &STUB_RUN, &STUB_DRAIN);
        assert!(app.dialog.is_none());
        assert!(has_cmd_line(&app, "kick alice"));
    }

    #[test]
    fn form_dangerous_submit_opens_confirm_not_executed() {
        let mut app = App::new();
        app.open_form(&KICK_FORM);
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
        run_action(&mut app, &STUB_RUN, &STUB_DRAIN, &Action::Run(vec!["disband"]));
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
    fn prefer_cli_truthy_from_effective_value() {
        let _guard = OPT_TEST_LOCK.lock().unwrap();
        set_session(Some("true"));
        set_global(None);
        let value = opt_get_effective(Some(&STUB_SESSION), &STUB_GLOBAL, "PREFER_COMMAND_LINE");
        assert!(is_truthy(&value));
        set_session(None);
    }
}
