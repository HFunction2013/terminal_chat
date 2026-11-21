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

use crate::ui_registry;
use anyhow::Result;
use cli_core_types::plugin_fn_types::ListRemoteDirFn;
use cli_core_types::{IsConnectedFn, RunCommandFn, SetInterruptedFn};
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, LazyLock, Mutex};
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
}

impl CommandWorker {
    /// 启动常驻 worker 线程；`run_command` 为裸 FFI 函数指针副本（`Send`/`Copy`）。
    fn spawn(run_command: RunCommandFn) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<(Vec<String>, bool)>();
        let running = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let running_w = running.clone();
        let result_w = result.clone();
        thread::spawn(move || {
            while let Ok((argv, silent)) = rx.recv() {
                running_w.store(true, Ordering::SeqCst);
                let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
                    argv.iter().map(|s| s.as_str().into()).collect::<Vec<_>>().into();
                let res = run_command(&args_ffi);
                *result_w.lock().unwrap() = Some(JobResult {
                    code: res.code,
                    message: res.message.to_string(),
                    argv,
                    silent,
                });
                running_w.store(false, Ordering::SeqCst);
            }
        });
        Self { tx, running, result }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 投递待执行命令；`silent` 标记该作业是否为后台回填型刷新。
    fn submit(&self, argv: Vec<String>, silent: bool) {
        let _ = self.tx.send((argv, silent));
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

/// `join_freq` 表单：freq 必填，password 可选。
static JOIN_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Join Frequency".to_string(),
    cmd: "join_freq".to_string(),
    prefill: "join_freq ".to_string(),
    fields: vec![
        FormField {
            label: "Frequency ID:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Password (optional):".to_string(),
            required: false,
            secret: true,
            kind: FieldKind::Plain,
        },
    ],
});

/// `workspace switch` 表单。
static SWITCH_WS_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Switch Workspace".to_string(),
    cmd: "workspace switch".to_string(),
    prefill: "workspace switch ".to_string(),
    fields: vec![FormField {
        label: "Workspace name:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// `workspace new` 表单：名称可选（服务端默认 `Workspace-xxx`）。
static NEW_WS_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "New Workspace".to_string(),
    cmd: "workspace new".to_string(),
    prefill: "workspace new ".to_string(),
    fields: vec![FormField {
        label: "Workspace name (optional):".to_string(),
        required: false,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// `create_freq` 表单：频点名必填，密码可选（掩码回显）；成员数等其余参数仍走命令行。
static CREATE_FREQ_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Create Frequency".to_string(),
    cmd: "create_freq".to_string(),
    prefill: "create_freq ".to_string(),
    fields: vec![
        FormField {
            label: "Frequency ID:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Password (optional):".to_string(),
            required: false,
            secret: true,
            kind: FieldKind::Plain,
        },
    ],
});

/// `msg` 表单：收件人可选（默认全员），消息必填。
static MSG_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Send Message".to_string(),
    cmd: "msg".to_string(),
    prefill: "msg -m ".to_string(),
    fields: vec![
        FormField {
            label: "Users (optional):".to_string(),
            required: false,
            secret: false,
            kind: FieldKind::Member,
        },
        FormField {
            label: "Message:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
    ],
});

/// `reply` 表单。
static REPLY_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Reply".to_string(),
    cmd: "reply".to_string(),
    prefill: "reply ".to_string(),
    fields: vec![FormField {
        label: "Message:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// `call` 表单。
static CALL_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Call User".to_string(),
    cmd: "call".to_string(),
    prefill: "call ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});

/// 单用户管理命令表单（mute/unmute/ignore/kick/ban）。
static MUTE_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Mute User".to_string(),
    cmd: "mute".to_string(),
    prefill: "mute ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static UNMUTE_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unmute User".to_string(),
    cmd: "unmute".to_string(),
    prefill: "unmute ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static IGNORE_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Ignore User".to_string(),
    cmd: "ignore".to_string(),
    prefill: "ignore ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static KICK_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Kick User".to_string(),
    cmd: "kick".to_string(),
    prefill: "kick ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static BAN_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Ban User".to_string(),
    cmd: "ban".to_string(),
    prefill: "ban ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});

/// 文件列表表单：目录可选。
static LS_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "List Remote Files".to_string(),
    cmd: "ls".to_string(),
    prefill: "ls ".to_string(),
    fields: vec![FormField {
        label: "Directory (optional):".to_string(),
        required: false,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});

/// 上传表单：本地路径必填，远端路径可选。
static UPLOAD_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Upload".to_string(),
    cmd: "upload".to_string(),
    prefill: "upload ".to_string(),
    fields: vec![
        FormField {
            label: "Local path:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::LocalPath,
        },
        FormField {
            label: "Remote path (optional):".to_string(),
            required: false,
            secret: false,
            kind: FieldKind::RemotePath,
        },
    ],
});

/// 下载表单：远端路径必填，本地路径可选。
static DOWNLOAD_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Download".to_string(),
    cmd: "download".to_string(),
    prefill: "download ".to_string(),
    fields: vec![
        FormField {
            label: "Remote path:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
        FormField {
            label: "Local path (optional):".to_string(),
            required: false,
            secret: false,
            kind: FieldKind::LocalPath,
        },
    ],
});

/// 删除远端文件表单。
static RM_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Remove Remote File".to_string(),
    cmd: "rm".to_string(),
    prefill: "rm ".to_string(),
    fields: vec![FormField {
        label: "Remote path:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});

/// 移动/重命名远端文件表单。
static MV_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Move Remote File".to_string(),
    cmd: "mv".to_string(),
    prefill: "mv ".to_string(),
    fields: vec![
        FormField {
            label: "From:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
        FormField {
            label: "To:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
    ],
});

/// 复制远端文件表单。
static CP_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Copy Remote File".to_string(),
    cmd: "cp".to_string(),
    prefill: "cp ".to_string(),
    fields: vec![
        FormField {
            label: "From:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
        FormField {
            label: "To:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::RemotePath,
        },
    ],
});

/// 加载插件表单。
static LOAD_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Load Plugin".to_string(),
    cmd: "load".to_string(),
    prefill: "load ".to_string(),
    fields: vec![FormField {
        label: "Plugin:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 卸载插件表单。
static UNLOAD_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unload Plugin".to_string(),
    cmd: "unload".to_string(),
    prefill: "unload ".to_string(),
    fields: vec![FormField {
        label: "Plugin name:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 本地 shell 命令表单。
static SHELL_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Local Shell".to_string(),
    cmd: "shell".to_string(),
    prefill: "shell ".to_string(),
    fields: vec![FormField {
        label: "Command:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 单用户管理命令表单（unban/unignore/op/deop/trop/user_info）。
static UNBAN_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unban User".to_string(),
    cmd: "unban".to_string(),
    prefill: "unban ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static UNIGNORE_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unignore User".to_string(),
    cmd: "unignore".to_string(),
    prefill: "unignore ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static OP_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Grant Moderator".to_string(),
    cmd: "op".to_string(),
    prefill: "op ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static DEOP_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Revoke Moderator".to_string(),
    cmd: "deop".to_string(),
    prefill: "deop ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static TROP_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Transfer Creator".to_string(),
    cmd: "trop".to_string(),
    prefill: "trop ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});
static USER_INFO_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "User Info".to_string(),
    cmd: "user_info".to_string(),
    prefill: "user_info ".to_string(),
    fields: vec![FormField {
        label: "User:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Member,
    }],
});

/// 远端文件系统命令表单。
static MKDIR_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Make Remote Directory".to_string(),
    cmd: "mkdir".to_string(),
    prefill: "mkdir ".to_string(),
    fields: vec![FormField {
        label: "Directory:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});
static CD_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Change Remote Directory".to_string(),
    cmd: "cd".to_string(),
    prefill: "cd ".to_string(),
    fields: vec![FormField {
        label: "Directory:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});
static CHECKSUM_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "File Checksum".to_string(),
    cmd: "checksum".to_string(),
    prefill: "checksum ".to_string(),
    fields: vec![FormField {
        label: "File:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});
static FILE_INFO_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "File Info".to_string(),
    cmd: "file_info".to_string(),
    prefill: "file_info ".to_string(),
    fields: vec![FormField {
        label: "File:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::RemotePath,
    }],
});

/// 定时消息表单：time 位置、message 走 `-m`、users 可选。
static SCHEDULE_MSG_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Schedule Message".to_string(),
    cmd: "schedule_msg".to_string(),
    prefill: "schedule_msg ".to_string(),
    fields: vec![
        FormField {
            label: "Time (+30s / HH:MM):".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Message:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Users (optional):".to_string(),
            required: false,
            secret: false,
            kind: FieldKind::Member,
        },
    ],
});
static CANCEL_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Cancel Task".to_string(),
    cmd: "cancel".to_string(),
    prefill: "cancel ".to_string(),
    fields: vec![FormField {
        label: "Task ID:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 加载/卸载命名空间表单。
static LOAD_NS_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Load Namespace".to_string(),
    cmd: "load namespace".to_string(),
    prefill: "load namespace ".to_string(),
    fields: vec![FormField {
        label: "Namespace:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});
static UNLOAD_NS_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unload Namespace".to_string(),
    cmd: "unload namespace".to_string(),
    prefill: "unload namespace ".to_string(),
    fields: vec![FormField {
        label: "Namespace:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 会话/全局选项表单。
static SET_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Set Session Option".to_string(),
    cmd: "set".to_string(),
    prefill: "set ".to_string(),
    fields: vec![
        FormField {
            label: "Key:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Value:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
    ],
});
static GET_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Get Session Option".to_string(),
    cmd: "get".to_string(),
    prefill: "get ".to_string(),
    fields: vec![FormField {
        label: "Key:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});
static SETG_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Set Global Option".to_string(),
    cmd: "setg".to_string(),
    prefill: "setg ".to_string(),
    fields: vec![
        FormField {
            label: "Key:".to_string(),
            required: true,
            secret: false,
            kind: FieldKind::Plain,
        },
        FormField {
            label: "Value (optional):".to_string(),
            required: false,
            secret: false,
            kind: FieldKind::Plain,
        },
    ],
});
static GETG_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Get Global Option".to_string(),
    cmd: "getg".to_string(),
    prefill: "getg ".to_string(),
    fields: vec![FormField {
        label: "Key:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});
static UNSET_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unset Session Option".to_string(),
    cmd: "unset".to_string(),
    prefill: "unset ".to_string(),
    fields: vec![FormField {
        label: "Key:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});
static UNSETG_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Unset Global Option".to_string(),
    cmd: "unsetg".to_string(),
    prefill: "unsetg ".to_string(),
    fields: vec![FormField {
        label: "Key:".to_string(),
        required: true,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 频点信息表单：频点可选（留空取当前）。
static FREQ_INFO_FORM: LazyLock<FormSpec> = LazyLock::new(|| FormSpec {
    title: "Frequency Info".to_string(),
    cmd: "freq_info".to_string(),
    prefill: "freq_info ".to_string(),
    fields: vec![FormField {
        label: "Frequency (optional):".to_string(),
        required: false,
        secret: false,
        kind: FieldKind::Plain,
    }],
});

/// 全部带参命令的表单规格；供菜单动作之外的输入框回车路径按命令名反查。
static FORM_SPECS: LazyLock<Vec<&'static FormSpec>> = LazyLock::new(|| {
    vec![
        &*JOIN_FORM,
        &*SWITCH_WS_FORM,
        &*NEW_WS_FORM,
        &*CREATE_FREQ_FORM,
        &*MSG_FORM,
        &*REPLY_FORM,
        &*CALL_FORM,
        &*MUTE_FORM,
        &*UNMUTE_FORM,
        &*IGNORE_FORM,
        &*KICK_FORM,
        &*BAN_FORM,
        &*LS_FORM,
        &*UPLOAD_FORM,
        &*DOWNLOAD_FORM,
        &*RM_FORM,
        &*MV_FORM,
        &*CP_FORM,
        &*LOAD_FORM,
        &*UNLOAD_FORM,
        &*SHELL_FORM,
        &*UNBAN_FORM,
        &*UNIGNORE_FORM,
        &*OP_FORM,
        &*DEOP_FORM,
        &*TROP_FORM,
        &*USER_INFO_FORM,
        &*MKDIR_FORM,
        &*CD_FORM,
        &*CHECKSUM_FORM,
        &*FILE_INFO_FORM,
        &*SCHEDULE_MSG_FORM,
        &*CANCEL_FORM,
        &*LOAD_NS_FORM,
        &*UNLOAD_NS_FORM,
        &*SET_FORM,
        &*GET_FORM,
        &*SETG_FORM,
        &*GETG_FORM,
        &*UNSET_FORM,
        &*UNSETG_FORM,
        &*FREQ_INFO_FORM,
    ]
});

/// 按 argv 反查表单规格：先匹配复合命令（前两个 token，如 `workspace switch`），
/// 再匹配单 token 命令；未注册返回 `None`。
fn form_spec_for(argv: &[String]) -> Option<&'static FormSpec> {
    if let Some(two) = argv.first_chunk::<2>() {
        let compound = format!("{} {}", two[0], two[1]);
        if let Some(spec) = FORM_SPECS.iter().find(|s| s.cmd == compound) {
            return Some(spec);
        }
    }
    let first = argv.first()?;
    FORM_SPECS.iter().find(|s| s.cmd == first.as_str()).copied()
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
    "about",
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
    "namespace",
    "notifications",
    "op",
    "plugin",
    "proxy",
    "pwd",
    "reconnect",
    "reply",
    "rm",
    "route",
    "run",
    "schedule",
    "schedule_msg",
    "sessions",
    "set",
    "setg",
    "shell",
    "show",
    "sleep",
    "status",
    "tasks",
    "tcpm",
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

/// 复合命令 → 第二词子命令名表（对齐 `cli-standard/commands/` 下各子命令 executor）。
const SUBCOMMANDS: &[(&str, &[&str])] = &[
    (
        "workspace",
        &["clone", "delete", "export", "import", "list", "new", "rename", "show", "switch"],
    ),
    (
        "show",
        &[
            "active_freqs",
            "active_users",
            "globals",
            "locals",
            "moderators",
            "options",
            "plugins",
            "variables",
        ],
    ),
    ("load", &["namespace", "plugin"]),
    ("unload", &["namespace", "plugin"]),
    ("namespace", &["list", "load", "unload"]),
    ("plugin", &["list", "load", "unload"]),
    ("create", &["alias", "macro"]),
    ("delete", &["alias", "macro"]),
    ("proxy", &["off", "on"]),
    ("tcpm", &["download", "install", "list", "search", "show", "uninstall", "update"]),
];

/// 查复合命令的子命令名表；非复合命令返回 `None`。
fn subcommands_of(cmd: &str) -> Option<&'static [&'static str]> {
    SUBCOMMANDS.iter().find(|(c, _)| *c == cmd).map(|(_, subs)| *subs)
}

/// 菜单定义：按 GUI 软件的大功能分组。
fn menu_groups() -> Vec<MenuGroup> {
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
            name: "Connection".to_string(),
            accel: 'C',
            items: vec![
                MenuItem {
                    label: "Connect".to_string(),
                    action: Action::Run(vec!["connect".to_string()]),
                },
                MenuItem {
                    label: "Reconnect".to_string(),
                    action: Action::Run(vec!["reconnect".to_string()]),
                },
                MenuItem {
                    label: "Disconnect".to_string(),
                    action: Action::Run(vec!["disconnect".to_string()]),
                },
                MenuItem {
                    label: "Whoami".to_string(),
                    action: Action::Run(vec!["whoami".to_string()]),
                },
                MenuItem {
                    label: "Status".to_string(),
                    action: Action::Run(vec!["status".to_string()]),
                },
                MenuItem {
                    label: "Route".to_string(),
                    action: Action::Run(vec!["route".to_string()]),
                },
            ],
        },
        MenuGroup {
            name: "Workspace".to_string(),
            accel: 'W',
            items: vec![
                MenuItem {
                    label: "List".to_string(),
                    action: Action::Run(vec!["workspace".to_string(), "list".to_string()]),
                },
                MenuItem {
                    label: "Switch...".to_string(),
                    action: Action::Form(Box::new((&*SWITCH_WS_FORM).clone())),
                },
                MenuItem {
                    label: "New...".to_string(),
                    action: Action::Form(Box::new((&*NEW_WS_FORM).clone())),
                },
                MenuItem {
                    label: "Show".to_string(),
                    action: Action::Run(vec!["workspace".to_string(), "show".to_string()]),
                },
            ],
        },
        MenuGroup {
            name: "Frequency".to_string(),
            accel: 'F',
            items: vec![
                MenuItem {
                    label: "Join...".to_string(),
                    action: Action::Form(Box::new((&*JOIN_FORM).clone())),
                },
                MenuItem {
                    label: "Create...".to_string(),
                    action: Action::Form(Box::new((&*CREATE_FREQ_FORM).clone())),
                },
                MenuItem {
                    label: "Leave".to_string(),
                    action: Action::Run(vec!["leave_freq".to_string()]),
                },
                MenuItem {
                    label: "Info...".to_string(),
                    action: Action::Form(Box::new((&*FREQ_INFO_FORM).clone())),
                },
                MenuItem {
                    label: "Background".to_string(),
                    action: Action::Run(vec!["background".to_string()]),
                },
                MenuItem {
                    label: "Foreground".to_string(),
                    action: Action::Run(vec!["foreground".to_string()]),
                },
                MenuItem {
                    label: "Away".to_string(),
                    action: Action::Run(vec!["away".to_string()]),
                },
                MenuItem {
                    label: "Notifications".to_string(),
                    action: Action::Run(vec!["notifications".to_string()]),
                },
                MenuItem {
                    label: "Sessions".to_string(),
                    action: Action::Run(vec!["sessions".to_string(), "-l".to_string()]),
                },
                MenuItem {
                    label: "Disband...".to_string(),
                    action: Action::Run(vec!["disband".to_string()]),
                },
                MenuItem {
                    label: "Active freqs".to_string(),
                    action: Action::Run(vec!["show".to_string(), "active_freqs".to_string()]),
                },
                MenuItem {
                    label: "Active users".to_string(),
                    action: Action::Run(vec!["show".to_string(), "active_users".to_string()]),
                },
            ],
        },
        MenuGroup {
            name: "Message".to_string(),
            accel: 'M',
            items: vec![
                MenuItem {
                    label: "Send...".to_string(),
                    action: Action::Form(Box::new((&*MSG_FORM).clone())),
                },
                MenuItem {
                    label: "Reply...".to_string(),
                    action: Action::Form(Box::new((&*REPLY_FORM).clone())),
                },
                MenuItem {
                    label: "Call...".to_string(),
                    action: Action::Form(Box::new((&*CALL_FORM).clone())),
                },
                MenuItem {
                    label: "Schedule...".to_string(),
                    action: Action::Form(Box::new((&*SCHEDULE_MSG_FORM).clone())),
                },
                MenuItem {
                    label: "Cancel Task...".to_string(),
                    action: Action::Form(Box::new((&*CANCEL_FORM).clone())),
                },
            ],
        },
        MenuGroup {
            name: "Moderation".to_string(),
            accel: 'O',
            items: vec![
                MenuItem {
                    label: "Mute...".to_string(),
                    action: Action::Form(Box::new((&*MUTE_FORM).clone())),
                },
                MenuItem {
                    label: "Unmute...".to_string(),
                    action: Action::Form(Box::new((&*UNMUTE_FORM).clone())),
                },
                MenuItem {
                    label: "Ignore...".to_string(),
                    action: Action::Form(Box::new((&*IGNORE_FORM).clone())),
                },
                MenuItem {
                    label: "Kick...".to_string(),
                    action: Action::Form(Box::new((&*KICK_FORM).clone())),
                },
                MenuItem {
                    label: "Ban...".to_string(),
                    action: Action::Form(Box::new((&*BAN_FORM).clone())),
                },
                MenuItem {
                    label: "Unban...".to_string(),
                    action: Action::Form(Box::new((&*UNBAN_FORM).clone())),
                },
                MenuItem {
                    label: "Unignore...".to_string(),
                    action: Action::Form(Box::new((&*UNIGNORE_FORM).clone())),
                },
                MenuItem {
                    label: "Op...".to_string(),
                    action: Action::Form(Box::new((&*OP_FORM).clone())),
                },
                MenuItem {
                    label: "Deop...".to_string(),
                    action: Action::Form(Box::new((&*DEOP_FORM).clone())),
                },
                MenuItem {
                    label: "Trop...".to_string(),
                    action: Action::Form(Box::new((&*TROP_FORM).clone())),
                },
                MenuItem {
                    label: "User Info...".to_string(),
                    action: Action::Form(Box::new((&*USER_INFO_FORM).clone())),
                },
            ],
        },
        MenuGroup {
            name: "Files".to_string(),
            accel: 'L',
            items: vec![
                MenuItem {
                    label: "List...".to_string(),
                    action: Action::Form(Box::new((&*LS_FORM).clone())),
                },
                MenuItem {
                    label: "Upload...".to_string(),
                    action: Action::Form(Box::new((&*UPLOAD_FORM).clone())),
                },
                MenuItem {
                    label: "Download...".to_string(),
                    action: Action::Form(Box::new((&*DOWNLOAD_FORM).clone())),
                },
                MenuItem {
                    label: "Tree".to_string(),
                    action: Action::Run(vec!["tree".to_string()]),
                },
                MenuItem {
                    label: "Remove...".to_string(),
                    action: Action::Form(Box::new((&*RM_FORM).clone())),
                },
                MenuItem {
                    label: "Move...".to_string(),
                    action: Action::Form(Box::new((&*MV_FORM).clone())),
                },
                MenuItem {
                    label: "Copy...".to_string(),
                    action: Action::Form(Box::new((&*CP_FORM).clone())),
                },
                MenuItem {
                    label: "Mkdir...".to_string(),
                    action: Action::Form(Box::new((&*MKDIR_FORM).clone())),
                },
                MenuItem {
                    label: "Cd...".to_string(),
                    action: Action::Form(Box::new((&*CD_FORM).clone())),
                },
                MenuItem {
                    label: "Checksum...".to_string(),
                    action: Action::Form(Box::new((&*CHECKSUM_FORM).clone())),
                },
                MenuItem {
                    label: "File Info...".to_string(),
                    action: Action::Form(Box::new((&*FILE_INFO_FORM).clone())),
                },
                MenuItem {
                    label: "Pwd (remote)".to_string(),
                    action: Action::Run(vec!["pwd".to_string()]),
                },
                MenuItem {
                    label: "Local Pwd".to_string(),
                    action: Action::Run(vec!["lpwd".to_string()]),
                },
                MenuItem {
                    label: "Local List".to_string(),
                    action: Action::Run(vec!["lls".to_string()]),
                },
                MenuItem {
                    label: "Local Cd...".to_string(),
                    action: Action::Run(vec!["lcd".to_string()]),
                },
            ],
        },
        MenuGroup {
            name: "Plugins".to_string(),
            accel: 'P',
            items: vec![
                MenuItem {
                    label: "Show".to_string(),
                    action: Action::Run(vec!["show".to_string(), "plugins".to_string()]),
                },
                MenuItem {
                    label: "Load...".to_string(),
                    action: Action::Form(Box::new((&*LOAD_FORM).clone())),
                },
                MenuItem {
                    label: "Unload...".to_string(),
                    action: Action::Form(Box::new((&*UNLOAD_FORM).clone())),
                },
                MenuItem {
                    label: "Load Namespace...".to_string(),
                    action: Action::Form(Box::new((&*LOAD_NS_FORM).clone())),
                },
                MenuItem {
                    label: "Unload Namespace...".to_string(),
                    action: Action::Form(Box::new((&*UNLOAD_NS_FORM).clone())),
                },
                MenuItem {
                    label: "Shell...".to_string(),
                    action: Action::Form(Box::new((&*SHELL_FORM).clone())),
                },
            ],
        },
        MenuGroup {
            name: "Settings".to_string(),
            accel: 'S',
            items: vec![
                MenuItem {
                    label: "Set...".to_string(),
                    action: Action::Form(Box::new((&*SET_FORM).clone())),
                },
                MenuItem {
                    label: "Get...".to_string(),
                    action: Action::Form(Box::new((&*GET_FORM).clone())),
                },
                MenuItem {
                    label: "Set Global...".to_string(),
                    action: Action::Form(Box::new((&*SETG_FORM).clone())),
                },
                MenuItem {
                    label: "Get Global...".to_string(),
                    action: Action::Form(Box::new((&*GETG_FORM).clone())),
                },
                MenuItem {
                    label: "Unset...".to_string(),
                    action: Action::Form(Box::new((&*UNSET_FORM).clone())),
                },
                MenuItem {
                    label: "Unset Global...".to_string(),
                    action: Action::Form(Box::new((&*UNSETG_FORM).clone())),
                },
                MenuItem {
                    label: "Options".to_string(),
                    action: Action::Run(vec!["show".to_string(), "options".to_string()]),
                },
                MenuItem {
                    label: "Locals".to_string(),
                    action: Action::Run(vec!["show".to_string(), "locals".to_string()]),
                },
            ],
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
        ui_registry::RegMenuAction::Form(rs) => Action::Form(Box::new(reg_spec_to_spec(rs))),
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

/// 注册表菜单启用闸门：菜单 / 表单整体迁移完成前保持关闭，宿主回退内置硬编码
/// 菜单；#150 删除硬编码时翻为 `true`。过渡期不做注册表与硬编码的合并补齐。
const REGISTRY_MENUS_ENABLED: bool = false;

/// 解析当前生效菜单：注册表启用且非空时用注册表条目，否则回退内置硬编码菜单。
fn resolve_groups(
    menu_fn: Option<&ui_registry::GetMenuItemsFn>,
    form_fn: Option<&ui_registry::GetFormSpecFn>,
) -> Vec<MenuGroup> {
    if !REGISTRY_MENUS_ENABLED {
        return menu_groups();
    }
    let registered = ui_registry::fetch_menu_items(menu_fn);
    if registered.is_empty() { menu_groups() } else { reg_groups_to_groups(registered, form_fn) }
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
    /// 侧栏 Sessions 栏目是否展开。
    sessions_open: bool,
    /// 侧栏 Members 栏目是否展开。
    members_open: bool,
    /// 侧栏 Plugins 栏目是否展开。
    plugins_open: bool,
    /// 远端目录列举回调（cli-core `remote_list_dir`）；缺失时远端路径不补全。
    remote_list_dir: Option<ListRemoteDirFn>,
    /// 连接态查询回调（cli-core `is_connected`）；缺失时视为未连接。
    is_connected: Option<IsConnectedFn>,
    /// 设置命令中断标志的回调（cli-core `set_interrupted`）；缺失时 Ctrl+C 不中断。
    set_interrupted: Option<SetInterruptedFn>,
    /// 已注册选项列举回调（cli-core `get_registered_options`）；缺失时选项 key 不补全。
    list_registered_options: Option<GetRegisteredOptionsFn>,
    /// 已注册选项 key 缓存（供选项参数位置补全）。
    registered_options: Vec<String>,
    /// 注册表追加的底部状态栏片段（插件 `on_init_plugin` 登记）；空时仅显示内置项。
    bottom_fragments: Vec<String>,
    /// 注册表侧栏栏目壳（插件登记）；空时回退内置硬编码三段。
    side_sections: Vec<ui_registry::RegSideSection>,
    /// 注册表侧栏条目模板（前缀符号 + 内容占位）；实际条目名由宿主快照回填。
    side_items: Vec<ui_registry::RegSideItem>,
    /// 注册表栏目壳折叠态，按段 id 存取；缺省取段壳 `default_open`。
    side_open: HashMap<String, bool>,
    /// 状态栏快照（来自全局选项）。
    username: String,
    server: String,
    port: String,
    quiet: bool,
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
            sessions_open: true,
            members_open: true,
            plugins_open: true,
            remote_list_dir: None,
            is_connected: None,
            set_interrupted: None,
            list_registered_options: None,
            registered_options: Vec::new(),
            bottom_fragments: Vec::new(),
            side_sections: Vec::new(),
            side_items: Vec::new(),
            side_open: HashMap::new(),
            username: String::new(),
            server: String::new(),
            port: String::new(),
            quiet: false,
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

    /// 判定当前活动缓冲的补全类别、前缀与首词命令（表单首词为空）。
    fn completion_context(&self) -> (FieldKind, String, String) {
        if let Some(Dialog::Form(form)) = &self.dialog {
            let kind = form.spec.fields[form.focus].kind;
            return (kind, form.values[form.focus].clone(), String::new());
        }
        let parts: Vec<&str> = self.input.split_whitespace().collect();
        match parts.as_slice() {
            [] => (FieldKind::Plain, String::new(), String::new()),
            [first] => {
                let cmd = (*first).to_string();
                // 首词后已敲空格：处于第一参数位置（尾空格被 split 丢弃，需按原始输入判断）。
                if self.input.ends_with(' ') {
                    let kind = if subcommands_of(&cmd).is_some() {
                        FieldKind::Subcommand
                    } else {
                        kind_for_command_arg(&cmd, 1)
                    };
                    (kind, String::new(), cmd)
                } else {
                    (FieldKind::Plain, cmd.clone(), cmd)
                }
            }
            _ => {
                let cmd = parts[0].to_string();
                let arg_index = parts.len() - 1; // 命令后第几个参数（1 起）
                let kind = if arg_index == 1 && subcommands_of(&cmd).is_some() {
                    FieldKind::Subcommand
                } else {
                    kind_for_command_parts(&parts)
                };
                (kind, parts[parts.len() - 1].to_string(), cmd)
            }
        }
    }

    /// 依据当前类别刷新补全候选：命令名 / 成员名 / 远端路径 / 本地路径。
    fn refresh_completions(&mut self) {
        let (kind, prefix, cmd) = self.completion_context();
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
            FieldKind::Subcommand => subcommands_of(&cmd)
                .unwrap_or(&[])
                .iter()
                .copied()
                .filter(|s| s.starts_with(&prefix))
                .map(String::from)
                .collect(),
            FieldKind::Member => {
                self.members.iter().filter(|m| m.starts_with(&prefix)).cloned().collect()
            }
            FieldKind::Plugin => self
                .plugins
                .iter()
                .filter(|p| p.name.starts_with(&prefix))
                .map(|p| p.name.clone())
                .collect(),
            FieldKind::Namespace => self
                .plugins
                .iter()
                .filter(|p| p.namespace.starts_with(&prefix))
                .map(|p| p.namespace.clone())
                .collect(),
            FieldKind::RemotePath => {
                let (parent, base) = split_path_prefix(&prefix);
                self.remote_candidates(parent, base)
            }
            FieldKind::LocalPath => {
                let (parent, base) = split_path_prefix(&prefix);
                local_candidates(parent, base)
            }
            FieldKind::OptionKey => {
                let needle = prefix.to_uppercase();
                self.registered_options
                    .iter()
                    .filter(|k| k.to_uppercase().starts_with(&needle))
                    .cloned()
                    .collect()
            }
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

/// 按完整命令词序列判定补全类别；识别二级子命令之后的插件名 / 命名空间参数位置。
fn kind_for_command_parts(parts: &[&str]) -> FieldKind {
    if parts.len() >= 3 {
        match (parts[0], parts[1]) {
            ("plugin", "load")
            | ("plugin", "unload")
            | ("load", "plugin")
            | ("unload", "plugin") => FieldKind::Plugin,
            ("namespace", "load")
            | ("namespace", "unload")
            | ("load", "namespace")
            | ("unload", "namespace") => FieldKind::Namespace,
            _ => kind_for_command_arg(parts[0], parts.len() - 1),
        }
    } else {
        kind_for_command_arg(parts[0], parts.len() - 1)
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
        // 选项命令：第 1 参为已注册选项 key。
        "set" | "setg" | "get" | "getg" | "unset" | "unsetg" => {
            if arg_index == 1 {
                FieldKind::OptionKey
            } else {
                FieldKind::NoCompletion
            }
        }
        // 成员命令：第 1 参为用户名/昵称。
        "call" | "kick" | "ban" | "unban" | "op" | "deop" | "trop" | "mute" | "unmute"
        | "ignore" | "unignore" | "user_info" => FieldKind::Member,
        // msg：第 1 参为接收成员列表，其后为自由文本消息。
        "msg" => {
            if arg_index == 1 {
                FieldKind::Member
            } else {
                FieldKind::NoCompletion
            }
        }
        // 其余命令的参数为状态词 / 主机端口 / 自由文本或无参，不补全成员名。
        _ => FieldKind::NoCompletion,
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
    registered_options: Option<&GetRegisteredOptionsFn>,
    set_interrupted: Option<&SetInterruptedFn>,
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
    app.list_registered_options = registered_options.copied();
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
) -> Result<()> {
    let groups = resolve_groups(get_menu_items, get_form_spec);
    let mut was_busy = false;

    loop {
        // 刷新状态栏快照：会话设置优先，未设置回退全局。
        app.username = opt_get_effective(get_session, get_option, "USERNAME");
        app.server = opt_get_effective(get_session, get_option, "SERVER");
        app.port = opt_get_effective(get_session, get_option, "PORT");
        app.quiet = is_quiet(&opt_get_effective(get_session, get_option, "QUIET"));
        app.prefer_cli =
            is_truthy(&opt_get_effective(get_session, get_option, "PREFER_COMMAND_LINE"));
        app.refresh_registered_options();

        // 取积压推送。
        let pushes: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
        for p in pushes {
            let text = p.to_string();
            app.note_push(&text);
            app.push_line(text, LineKind::Push);
        }

        // 后台命令实时输出回流：逐帧取走 `print_content` 暂存的行，追加到聊天记录，
        // 同时累积文本供命令完成后回填成员侧栏。
        let drained: Vec<safer_ffi::String> = unsafe { output_drain() }.into();
        for d in drained {
            let text = d.to_string();
            if !app.silent_refresh {
                app.push_line(text.clone(), LineKind::Result);
            }
            app.worker_output.push_str(&text);
            app.worker_output.push('\n');
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

    if app.side_sections.is_empty() {
        draw_sidebar_legacy(ctx, app);
    } else {
        draw_sidebar_registry(ctx, app);
    }

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

/// 过渡期回退：内置硬编码三段侧栏（Session / Members / Plugin）。
///
/// 注册表段壳为空时使用；待 cli-standard 侧栏迁移完成、#150 删除硬编码后移除。
fn draw_sidebar_legacy(ctx: &mut Context, app: &mut App) {
    let sessions_glyph = if app.sessions_open { "#" } else { ">" };
    if ctx.row_button(
        "sec-freq",
        &section_header_text("Session", sessions_glyph),
        app.menubar_bg,
        app.menubar_fg,
    ) {
        app.sessions_open = !app.sessions_open;
    }
    if app.sessions_open {
        render_sessions_snapshot(ctx, app);
    }

    let members_glyph = if app.members_open { "#" } else { ">" };
    if ctx.row_button(
        "sec-members",
        &section_header_text("Members", members_glyph),
        app.menubar_bg,
        app.menubar_fg,
    ) {
        app.members_open = !app.members_open;
    }
    if app.members_open {
        render_members_snapshot(ctx, app);
    }

    let plugins_glyph = if app.plugins_open { "#" } else { ">" };
    if ctx.row_button(
        "sec-plugins",
        &section_header_text("Plugin", plugins_glyph),
        app.menubar_bg,
        app.menubar_fg,
    ) {
        app.plugins_open = !app.plugins_open;
    }
    if app.plugins_open {
        render_plugins_snapshot(ctx, app);
    }
}

/// 注册表驱动侧栏：遍历插件登记的栏目壳，按 `source` 取宿主快照回填条目。
fn draw_sidebar_registry(ctx: &mut Context, app: &mut App) {
    let sections = app.side_sections.clone();
    for section in &sections {
        let open = *app.side_open.get(&section.id).unwrap_or(&section.default_open);
        let glyph = if open { "#" } else { ">" };
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
        // 条目模板（前缀符号 + 内容占位）按栏目归属过滤；占位 ↔ 快照键的回填
        // 语义随 cli-standard 侧栏迁移（#149）定稿，本轮先按 source 复用快照渲染。
        let _templates: Vec<&ui_registry::RegSideItem> =
            app.side_items.iter().filter(|item| item.section == section.id).collect();
        match section.source.as_str() {
            "sessions" => render_sessions_snapshot(ctx, app),
            "members" => render_members_snapshot(ctx, app),
            "plugins" => render_plugins_snapshot(ctx, app),
            _ => {
                ctx.label("none", "  (none)");
            }
        }
    }
}

/// 会话快照：当前会话前缀 `*` 整行绿色；其余缩进列出。空快照显示当前频点占位。
fn render_sessions_snapshot(ctx: &mut Context, app: &mut App) {
    if app.sessions.is_empty() {
        let freq = app.current_freq.clone().unwrap_or_else(|| "(not joined)".to_string());
        ctx.label("freq", &format!("  {freq}"));
        return;
    }
    ctx.list_begin("sidebar-sessions");
    let sessions = app.sessions.clone();
    for s in &sessions {
        let name = s.name.clone();
        let sel = if s.current {
            ctx.styled_list_item_begin();
            ctx.styled_label_set_foreground(ctx.indexed(IndexedColor::Green));
            ctx.styled_label_add_text(&format!("* {name}"));
            ctx.styled_list_item_end(false)
        } else {
            ctx.list_item(false, &format!("  {name}"))
        };
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }
    }
    ctx.list_end();
}

/// 成员快照：缩进圆点列出成员名；空快照显示 `(none)`。
fn render_members_snapshot(ctx: &mut Context, app: &mut App) {
    if app.members.is_empty() {
        ctx.label("none", "  (none)");
        return;
    }
    ctx.list_begin("sidebar-members");
    let members = app.members.clone();
    for m in &members {
        let name = m.clone();
        let sel = ctx.list_item(false, &format!("  • {name}"));
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }
    }
    ctx.list_end();
}

/// 插件快照：第一行插件名，第二行命令命名空间；双击分别插入对应名字。
fn render_plugins_snapshot(ctx: &mut Context, app: &mut App) {
    if app.plugins.is_empty() {
        ctx.label("plugin-none", "  (none)");
        return;
    }
    ctx.list_begin("sidebar-plugins");
    let plugins = app.plugins.clone();
    for p in &plugins {
        let name = p.name.clone();
        let sel = ctx.list_item(false, &format!("  • {name}"));
        if let ListSelection::Activated = sel {
            app.append_entry_to_input(&name);
        }
    }
    for p in &plugins {
        let ns = p.namespace.clone();
        let sel = ctx.list_item(false, &format!("  : {ns}"));
        if let ListSelection::Activated = sel {
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
    let mut text = format!("{who}@{host} | freq {freq} | {quiet} | {}", now_ts());
    for frag in &app.bottom_fragments {
        text.push_str(&format!(" | {frag}"));
    }

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
        && let Some(spec) = form_spec_for(argv)
    {
        app.open_form(spec);
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
            let lines: Vec<String> = builtin_help_lines().into_iter().map(String::from).collect();
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

/// 内置 / 常用命令帮助文本。
fn builtin_help_lines() -> Vec<&'static str> {
    vec![
        "Built-in: exit/quit/q  quit | clear  clear log | version  version | build_info  build info",
        "          copyright  copyright | license  license | coffee  coffee | saying  fortune | help  this help",
        "Commands: connect  connect | msg -m <text>  send message | join_freq <freq> [password] | create_freq <freq>",
        "          show active_users | show active_freqs | workspace list | ls | upload | download",
    ]
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
    // 插件 / 命名空间装载或卸载成功后，重拉插件列表刷新侧栏分组。
    if success && matches!(cmd, "plugin" | "namespace" | "load" | "unload") {
        app.pending_plugins = true;
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
        let mut app = App::new();
        app.input = "con".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "connect"));
    }

    #[test]
    fn completes_member_prefix_after_command() {
        let mut app = App::new();
        app.members = vec!["alice".to_string().into(), "bob".to_string().into()];
        app.input = "msg al".into();
        app.refresh_completions();
        assert_eq!(app.completions, vec!["alice".to_string()]);
    }

    #[test]
    fn completes_plugin_name_after_plugin_load_or_unload() {
        let mut app = App::new();
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
        let mut app = App::new();
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
    fn legacy_load_unload_arg_classifies_plugin_or_namespace() {
        assert_eq!(kind_for_command_parts(&["load", "plugin", "x"]), FieldKind::Plugin);
        assert_eq!(kind_for_command_parts(&["unload", "plugin", "x"]), FieldKind::Plugin);
        assert_eq!(kind_for_command_parts(&["load", "namespace", "x"]), FieldKind::Namespace);
        assert_eq!(kind_for_command_parts(&["unload", "namespace", "x"]), FieldKind::Namespace);
    }

    #[test]
    fn option_key_arg_is_classified() {
        assert_eq!(kind_for_command_arg("set", 1), FieldKind::OptionKey);
        assert_eq!(kind_for_command_arg("setg", 1), FieldKind::OptionKey);
        assert_eq!(kind_for_command_arg("get", 1), FieldKind::OptionKey);
        assert_eq!(kind_for_command_arg("unsetg", 1), FieldKind::OptionKey);
        // 第 2 参（值）与自由文本消息没有可补全来源，不弹候选。
        assert_eq!(kind_for_command_arg("set", 2), FieldKind::NoCompletion);
        assert_eq!(kind_for_command_arg("msg", 1), FieldKind::Member);
        assert_eq!(kind_for_command_arg("msg", 2), FieldKind::NoCompletion);
    }

    #[test]
    fn no_command_names_after_command_without_arg_source() {
        let mut app = App::new();
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
        let mut app = App::new();
        app.input = "sta".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "status"));
    }

    #[test]
    fn unload_is_in_first_word_command_completion() {
        let mut app = App::new();
        app.input = "un".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "unload"));
    }

    #[test]
    fn completes_registered_option_prefix_case_insensitive() {
        let mut app = App::new();
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
        let mut app = App::new();
        app.input = "con".into();
        app.refresh_completions();
        let first = app.completions[0].clone();
        app.accept_completion();
        // 主命令行接受首词候选后补空格，进入第一参数位置。
        assert_eq!(app.input, format!("{first} "));
    }

    #[test]
    fn completes_subcommand_after_compound_prefix() {
        let mut app = App::new();
        app.input = "workspace l".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "list"));
        // 第二词位置不应回退到成员名补全。
        assert!(!app.completions.iter().any(|c| c == "alice"));
    }

    #[test]
    fn completes_all_subcommands_after_trailing_space() {
        let mut app = App::new();
        app.input = "show ".into();
        app.refresh_completions();
        assert!(app.completions.iter().any(|c| c == "active_users"));
        assert!(app.completions.iter().any(|c| c == "options"));
        assert_eq!(
            app.completions,
            subcommands_of("show").unwrap().iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn accept_subcommand_advances_past_separator() {
        let mut app = App::new();
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
        let mut app = App::new();
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
        let mut app = App::new();
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
        let with_pw = form_argv(&*JOIN_FORM, &["abc".into(), "secret".into()]);
        assert_eq!(with_pw, vec!["join_freq".to_string(), "abc".to_string(), "secret".to_string()]);

        let no_pw = form_argv(&*JOIN_FORM, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["join_freq".to_string(), "abc".to_string()]);
    }

    #[test]
    fn form_argv_create_freq_omits_empty_password() {
        let with_pw = form_argv(&*CREATE_FREQ_FORM, &["abc".into(), "secret".into()]);
        assert_eq!(
            with_pw,
            vec!["create_freq".to_string(), "abc".to_string(), "secret".to_string()]
        );

        let no_pw = form_argv(&*CREATE_FREQ_FORM, &["abc".into(), "".into()]);
        assert_eq!(no_pw, vec!["create_freq".to_string(), "abc".to_string()]);
    }

    #[test]
    fn open_form_initializes_focus_and_values() {
        let mut app = App::new();
        app.open_form(&*JOIN_FORM);
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
        app.open_form(&*JOIN_FORM);
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
        // 命令行偏好：退回输入框预填，不弹窗、不执行。
        let before = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.prefer_cli = true;
        app.is_connected = Some(STUB_CONNECTED);
        run_action(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &Action::Form(Box::new((&*JOIN_FORM).clone())),
        );
        assert_eq!(app.input, JOIN_FORM.prefill.as_str());
        assert!(app.dialog.is_none());
        // 预填路径绝不触发命令执行。
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before);

        // 默认：弹出表单收集参数，同样不在菜单点击时直接执行。
        let before2 = RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
        run_action(
            &mut app,
            &STUB_RUN,
            &STUB_DRAIN,
            &Action::Form(Box::new((&*JOIN_FORM).clone())),
        );
        assert!(app.dialog.is_some());
        assert!(app.input.is_empty());
        assert_eq!(RUN_CALLS.load(std::sync::atomic::Ordering::SeqCst), before2);
    }

    #[test]
    fn form_spec_for_matches_single_and_compound_commands() {
        let upload = vec!["upload".to_string()];
        assert_eq!(form_spec_for(&upload).unwrap().cmd, "upload");

        let switch_ws = vec!["workspace".to_string(), "switch".to_string()];
        assert_eq!(form_spec_for(&switch_ws).unwrap().cmd, "workspace switch");

        // 只有复合命令的首 token 时不应误匹配。
        let ws_only = vec!["workspace".to_string()];
        assert!(form_spec_for(&ws_only).is_none());

        let bogus = vec!["definitely_not_a_command".to_string()];
        assert!(form_spec_for(&bogus).is_none());
    }

    #[test]
    fn missing_arg_command_opens_form_when_not_prefer_cli() {
        let mut app = App::new();
        app.is_connected = Some(STUB_CONNECTED);
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
            &Action::Form(Box::new((&*JOIN_FORM).clone())),
        );
        assert!(app.dialog.is_none());
        assert!(app.input.is_empty());
    }

    #[test]
    fn form_argv_msg_splits_users_and_flags_message() {
        let argv = form_argv(&*MSG_FORM, &["alice bob".into(), "hello".into()]);
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
        let argv = form_argv(&*MSG_FORM, &["".into(), "hi".into()]);
        assert_eq!(argv, vec!["msg".to_string(), "-m".to_string(), "hi".to_string()]);
    }

    #[test]
    fn form_argv_two_arg_commands() {
        assert_eq!(
            form_argv(&*MV_FORM, &["a".into(), "b".into()]),
            vec!["mv".to_string(), "a".to_string(), "b".to_string()]
        );
        assert_eq!(
            form_argv(&*CP_FORM, &["x".into(), "y".into()]),
            vec!["cp".to_string(), "x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn form_argv_workspace_switch_and_plugin_load() {
        assert_eq!(
            form_argv(&*SWITCH_WS_FORM, &["dev".into()]),
            vec!["workspace".to_string(), "switch".to_string(), "dev".to_string()]
        );
        assert_eq!(
            form_argv(&*LOAD_FORM, &["foo".into()]),
            vec!["load".to_string(), "plugin".to_string(), "foo".to_string()]
        );
        assert_eq!(
            form_argv(&*UNLOAD_FORM, &["foo".into()]),
            vec!["unload".to_string(), "plugin".to_string(), "foo".to_string()]
        );
    }

    #[test]
    fn form_argv_shell_splits_command_line() {
        let argv = form_argv(&*SHELL_FORM, &["ls -la".into()]);
        assert_eq!(argv, vec!["shell".to_string(), "ls".to_string(), "-la".to_string()]);
    }

    #[test]
    fn form_argv_schedule_msg_flags_message_and_users() {
        let argv = form_argv(&*SCHEDULE_MSG_FORM, &["+5m".into(), "hi".into(), "alice bob".into()]);
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
            form_argv(&*LOAD_NS_FORM, &["net".into()]),
            vec!["load".to_string(), "namespace".to_string(), "net".to_string()]
        );
        assert_eq!(
            form_argv(&*UNLOAD_NS_FORM, &["net".into()]),
            vec!["unload".to_string(), "namespace".to_string(), "net".to_string()]
        );
    }

    #[test]
    fn form_argv_settings_key_value() {
        assert_eq!(
            form_argv(&*SET_FORM, &["k".into(), "v".into()]),
            vec!["set".to_string(), "k".to_string(), "v".to_string()]
        );
        // setg 的 value 可选：留空时只带 key。
        assert_eq!(
            form_argv(&*SETG_FORM, &["k".into(), "".into()]),
            vec!["setg".to_string(), "k".to_string()]
        );
        assert_eq!(form_argv(&*GET_FORM, &["k".into()]), vec!["get".to_string(), "k".to_string()]);
    }

    #[test]
    fn form_argv_extra_single_arg_commands() {
        assert_eq!(
            form_argv(&*UNBAN_FORM, &["u@h".into()]),
            vec!["unban".to_string(), "u@h".to_string()]
        );
        assert_eq!(
            form_argv(&*OP_FORM, &["u@h".into()]),
            vec!["op".to_string(), "u@h".to_string()]
        );
        assert_eq!(
            form_argv(&*MKDIR_FORM, &["docs".into()]),
            vec!["mkdir".to_string(), "docs".to_string()]
        );
        assert_eq!(
            form_argv(&*CANCEL_FORM, &["3".into()]),
            vec!["cancel".to_string(), "3".to_string()]
        );
        assert_eq!(
            form_argv(&*UNSET_FORM, &["k".into()]),
            vec!["unset".to_string(), "k".to_string()]
        );
        assert_eq!(
            form_argv(&*UNSETG_FORM, &["k".into()]),
            vec!["unsetg".to_string(), "k".to_string()]
        );
        // 可选频点留空：只发命令本身。
        assert_eq!(form_argv(&*FREQ_INFO_FORM, &["".into()]), vec!["freq_info".to_string()]);
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
    fn menu_groups_place_entries_in_correct_group() {
        let groups = menu_groups();
        let find = |name: &str| groups.iter().find(|g| g.name == name).expect(name);
        let labels = |g: &MenuGroup| g.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>();

        // File: Exit only; Connection must not hold Exit.
        assert_eq!(labels(find("File")), vec!["Exit".to_string()]);
        assert!(!labels(find("Connection")).contains(&"Exit".to_string()));

        // Connection: presence/session queries.
        let conn = labels(find("Connection"));
        for label in ["Whoami", "Status", "Route"] {
            assert!(conn.contains(&label.to_string()), "Connection should hold {label}");
        }

        // Frequency: disband lifecycle + online queries.
        let freq = labels(find("Frequency"));
        assert!(freq.contains(&"Disband...".to_string()));
        assert!(freq.contains(&"Active freqs".to_string()));
        assert!(freq.contains(&"Active users".to_string()));
        for label in ["Foreground", "Away", "Notifications"] {
            assert!(freq.contains(&label.to_string()), "Frequency should hold {label}");
        }

        // Files: remote pwd + local filesystem entries.
        let files = labels(find("Files"));
        for label in ["Pwd (remote)", "Local Pwd", "Local List", "Local Cd..."] {
            assert!(files.contains(&label.to_string()), "Files should hold {label}");
        }

        // Moderation: a single Ignore, no Disband.
        let moderation = labels(find("Moderation"));
        assert_eq!(moderation.iter().filter(|l| l.as_str() == "Ignore...").count(), 1);
        assert!(!moderation.contains(&"Disband...".to_string()));

        // Settings: option inspection; Plugins: local shell.
        let settings = labels(find("Settings"));
        assert!(settings.contains(&"Options".to_string()));
        assert!(settings.contains(&"Locals".to_string()));
        assert!(labels(find("Plugins")).contains(&"Shell...".to_string()));

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
        assert_eq!(kind_for_command_arg("msg", 2), FieldKind::NoCompletion);
        assert_eq!(kind_for_command_arg("kick", 1), FieldKind::Member);
        // 状态词 / 自由文本 / 无参命令不得补全成员名。
        assert_eq!(kind_for_command_arg("away", 1), FieldKind::NoCompletion);
        assert_eq!(kind_for_command_arg("reply", 1), FieldKind::NoCompletion);
        assert_eq!(kind_for_command_arg("status", 1), FieldKind::NoCompletion);
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
        app.open_form(&*KICK_FORM);
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
        let groups = menu_groups();

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
        assert!(!tui.testing_find("sec-members").is_empty());
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
        let groups = menu_groups();
        app.sessions = vec![SessionEntry { name: "1.0".to_string(), current: true }];
        app.members = vec!["alice".to_string(), "bob".to_string()];

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

        // Default: both sections expanded, glyph `#`, lists rendered.
        let sec_freq = tui.testing_find("sec-freq");
        assert!(sec_freq[0].text.contains('#'), "session header expanded: {:?}", sec_freq[0].text);
        assert!(
            sec_freq[0].width >= 20,
            "header row stretches across the sidebar: width {}",
            sec_freq[0].width
        );
        assert!(!tui.testing_find("sidebar-sessions").is_empty(), "session list shown");
        assert!(!tui.testing_find("sidebar-members").is_empty(), "member list shown");

        // Click the Session header row to collapse it.
        let center = row_center(&sec_freq[0]);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_freq = tui.testing_find("sec-freq");
        assert!(sec_freq[0].text.contains('>'), "session header collapsed: {:?}", sec_freq[0].text);
        assert!(
            tui.testing_find("sidebar-sessions").is_empty(),
            "session list hidden after collapse"
        );
        // Members section is untouched and still expanded.
        assert!(!tui.testing_find("sidebar-members").is_empty(), "member list still shown");

        // Click again to re-expand.
        let center = row_center(&sec_freq[0]);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_freq = tui.testing_find("sec-freq");
        assert!(
            sec_freq[0].text.contains('#'),
            "session header re-expanded: {:?}",
            sec_freq[0].text
        );
        assert!(!tui.testing_find("sidebar-sessions").is_empty(), "session list restored");

        // The Members header toggles independently.
        let sec_members = tui.testing_find("sec-members");
        let center = row_center(&sec_members[0]);
        click_at(&mut tui, &mut app, center, input::InputMouseState::Left);
        click_at(&mut tui, &mut app, center, input::InputMouseState::None);

        let sec_members = tui.testing_find("sec-members");
        assert!(
            sec_members[0].text.contains('>'),
            "members header collapsed: {:?}",
            sec_members[0].text
        );
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
        let groups = menu_groups();
        app.side_sections = vec![ui_registry::RegSideSection {
            id: "members".to_string(),
            title: "Members".to_string(),
            default_open: true,
            source: "members".to_string(),
        }];
        app.side_items = vec![ui_registry::RegSideItem {
            section: "members".to_string(),
            prefix: " • ".to_string(),
            label: "{member}".to_string(),
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();

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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
        let groups = menu_groups();
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
