use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use clap::Command;
use clap_complete::engine::complete;
use cli_core_types::{
    GetAllPluginsFn, GetCurrentLangFn, IsConnectedFn, IsInCmdFn, LoadPluginFn, PluginMetadata,
    RunCommandFn, SetCurrentFreqFn, SetInterruptedFn, plugin_fn_types::ListRemoteDirFn,
};
#[cfg(not(debug_assertions))]
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
};
use figlet_rs::FIGlet;
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(unix)]
use libc::{SIGTSTP, signal};
use libloading::{Library, Symbol};
use lolcat::{Config, Printer, choose_color_mode, initial_offset};
use rand::Rng;
use rustyline::{
    Context, Editor, ExternalPrinter, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};
use safer_ffi::prelude::*;
use sha2::{Digest, Sha256};
use shell_words::split;
use std::{
    ffi::OsString,
    io::{self, IsTerminal, stdout},
    mem::ManuallyDrop,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};
use which_dylib::{FindError, FindLibBuilder};

const FORTUNE_TEXT: &str = include_str!("../assets/fortune-people.txt");
static CLI_CORE: OnceLock<Library> = OnceLock::new();
fn get_cli_core() -> &'static Library {
    match FindLibBuilder::new().find_result("cli_core") {
        Ok(s) => {
            let lib_path = s.to_str().expect("Failed to convert `PathBuf` to `&str`");

            CLI_CORE.get_or_init(|| unsafe {
                Library::new(lib_path)
                    .unwrap_or_else(|e| panic!("Failed to load library '{}': {}", lib_path, e))
            })
        }
        Err(e) => match e {
            FindError::NotFound(s) => panic!("NotFoundError: {}", s),
            FindError::Ambiguous(v) => panic!("AmbiguousError: {:?}", v),
        },
    }
}

/// 非 panic 路径定位 cli-core 动态库；测试 / 旧核心环境缺失时返回 `None`，
/// 供语言探测等可选符号使用，不阻断启动与单测。
fn try_get_cli_core() -> Option<&'static Library> {
    static LIB: OnceLock<Option<Library>> = OnceLock::new();
    LIB.get_or_init(|| {
        FindLibBuilder::new().find("cli_core").and_then(|p| unsafe { Library::new(p).ok() })
    })
    .as_ref()
}

/// core `get_current_lang` 符号缓存；旧核心无此符号时为 `None`，回退英文。
static CURRENT_LANG_FN: OnceLock<Option<GetCurrentLangFn>> = OnceLock::new();

fn current_lang_fn() -> Option<GetCurrentLangFn> {
    *CURRENT_LANG_FN.get_or_init(|| {
        let lib = try_get_cli_core()?;
        unsafe { lib.get::<GetCurrentLangFn>(b"get_current_lang\0").ok().map(|s| *s) }
    })
}

/// 宿主翻译辅助：经 core `get_current_lang` 取当前语言序号（`1`→中文，其余→英文），
/// 再查编译期词典；符号缺失回退英文。REPL 与 TUI 两条路径共用，`setg LANGUAGE`
/// 后由 core 现场解析，下次调用即取到新语言。
pub(crate) fn tr(key: &str) -> &str {
    let lang = match current_lang_fn() {
        Some(f) => {
            let mut ord: i32 = 0;
            let _ = f(Out::from(&mut ord));
            match ord {
                1 => crate::i18n::Lang::Zh,
                _ => crate::i18n::Lang::En,
            }
        }
        None => crate::i18n::Lang::En,
    };
    crate::i18n::t_lang(lang, key)
}

/// cli-core 导出：取走异步推送队列（safer_ffi Vec<String>，C ABI）。
type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::Vec<safer_ffi::String>;

/// cli-core 导出：注册终端挂起 / 恢复钩子（参数为函数地址，0 表示未注册）。
type SetHooksFn = extern "C" fn(i64, i64);

/// 注册终端挂起 / 恢复钩子到 cli-core；符号缺失（旧核心）时静默回退。
fn register_terminal_hooks(suspend: *const (), resume: *const ()) {
    let lib = get_cli_core();
    if let Ok(set_hooks) = unsafe { lib.get::<SetHooksFn>(b"tc_cli_set_terminal_hooks") } {
        set_hooks(suspend as i64, resume as i64);
    }
}

/// TUI 挂起：离开 alt 屏并恢复进入 TUI 前的原始终端态，交给外部子进程。
extern "C" fn tui_terminal_suspend() -> cli_core_types::Result {
    edit_tui::sys::suspend_terminal();
    cli_core_types::Result::success()
}

/// TUI 恢复：重切 raw 模式并重进 alt 屏；下一帧由事件循环整屏重绘。
extern "C" fn tui_terminal_resume() -> cli_core_types::Result {
    let _ = edit_tui::sys::resume_terminal();
    cli_core_types::Result::success()
}

/// REPL 挂起：离开 alt 屏并显示光标，把终端交给外部子进程。
extern "C" fn repl_terminal_suspend() -> cli_core_types::Result {
    #[cfg(not(debug_assertions))]
    {
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
    let _ = execute!(stdout(), Show);
    cli_core_types::Result::success()
}

/// REPL 恢复：重进 alt 屏并隐藏光标。
extern "C" fn repl_terminal_resume() -> cli_core_types::Result {
    #[cfg(not(debug_assertions))]
    {
        let _ = execute!(stdout(), EnterAlternateScreen);
    }
    let _ = execute!(stdout(), Hide);
    cli_core_types::Result::success()
}

/// cli-core 导出：阻塞等待推送队列非空（毫秒超时），返回 1 有待取项 / 0 超时。
type PushWaitFn = unsafe extern "C" fn(u32) -> i32;

fn install_ctrlc_handler() {
    let lib = get_cli_core();

    let is_in_cmd: Symbol<IsInCmdFn> =
        unsafe { lib.get(b"is_in_cmd") }.expect("Failed to find symbol 'is_in_cmd'");
    let set_interrupted: Symbol<SetInterruptedFn> =
        unsafe { lib.get(b"set_interrupted") }.expect("Failed to find symbol 'set_interrupted'");

    ctrlc::set_handler(move || {
        let _ = set_interrupted(true);
        if is_in_cmd().code != 0 {
            #[cfg(not(debug_assertions))]
            let _ = execute!(stdout(), LeaveAlternateScreen);
            let _ = execute!(stdout(), Show);
            std::process::exit(0);
        }
        let _ = eprintln!("{}", crate::tr("repl.interrupt_hint"));
    })
    .expect("failed to install Ctrl+C handler");
}

#[allow(dead_code)] // `t` / `detect_lang_env` 保留供测试与兜底，生产路径走 `tr()`。
mod i18n {
    include!(concat!(env!("OUT_DIR"), "/i18n_cli.rs"));
}
mod tasks;
mod tui;
mod ui_registry;
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// 选项命令在 key 位置的补全候选：仅当首词为选项命令且处于第 1 参（key）位置时，
/// 按大小写不敏感前缀过滤已注册选项。（仅测试沿用旧口径断言，生产路径走 core 表驱动。）
#[cfg(test)]
fn option_candidates(
    options: &[String],
    cmd: Option<&str>,
    arg_index: usize,
    prefix: &str,
) -> Vec<String> {
    let is_option_cmd = matches!(
        cmd,
        Some("set") | Some("setg") | Some("get") | Some("getg") | Some("unset") | Some("unsetg")
    );
    if !is_option_cmd || arg_index != 1 {
        return Vec::new();
    }
    let needle = prefix.to_uppercase();
    options.iter().filter(|o| o.to_uppercase().starts_with(&needle)).cloned().collect()
}

/// 构造 REPL 命令回显行，对齐 TUI 的 `tc> <cmd>` 口径。
fn repl_echo_line(input: &str) -> String {
    format!("tc> {input}")
}

/// rustyline Helper：桥接 `clap_complete`，并叠加 core 命令表 / 参数规格补全。
struct ClapHelper {
    cli: Command,
    /// 已注册选项 key 缓存（符号缺失时的回退；正常路径由 `get_registered_options` 实时拉取）。
    options: Vec<String>,
    /// 已加载插件快照缓存 `(name, namespace)`（符号缺失时的回退；正常路径由
    /// `get_all_plugins` 实时拉取，避免卸载后残留旧候选）。
    plugins: Vec<(String, String)>,
    /// core 命令表查询与自定义概念回调（可选符号，缺失时对应补全退化）。
    list_commands: Option<tui::ListCommandsFn>,
    list_subcommands: Option<tui::ListSubcommandsFn>,
    arg_spec: Option<tui::ArgSpecFn>,
    complete_arg: Option<tui::CompleteArgHostFn>,
    remote_list_dir: Option<ListRemoteDirFn>,
    /// 实时列举已加载插件（供插件名 / 命名空间参数位置补全，卸载后立即生效）。
    get_all_plugins: Option<GetAllPluginsFn>,
    /// 实时列举已注册选项 key（供选项参数位置补全，卸载后立即生效）。
    get_registered_options: Option<tui::GetRegisteredOptionsFn>,
}

impl Helper for ClapHelper {}

impl Completer for ClapHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];

        let args = match split(before_cursor) {
            Ok(a) => a,
            Err(_) => return Ok((pos, vec![])),
        };

        let trailing = before_cursor.ends_with(' ');
        let committed: Vec<String>;
        let prefix: String;
        if trailing {
            committed = args.clone();
            prefix = String::new();
        } else {
            committed = args[..args.len().saturating_sub(1)].to_vec();
            prefix = args.last().map(|s| s.to_string()).unwrap_or_default();
        }
        // 命令后参数序号（1 起），与 core `arg_spec` 口径一致。
        let arg_index = committed.len();

        let mut cli = self.cli.clone();

        // 首词命令名只取 core 实时表：clap 命令树在启动时构建，卸载插件后不重建，
        // 沿用其候选会残留已卸载插件的命令名。非首词位置仍保留 clap 的基础候选。
        let mut pairs: Vec<Pair> = if committed.is_empty() {
            Vec::new()
        } else {
            let args_os: Vec<OsString> = std::iter::once("tc-cli".to_string())
                .chain(args.clone())
                .map(OsString::from)
                .collect();
            let candidates =
                complete(&mut cli, args_os, arg_index, Some(Path::new("."))).unwrap_or_default();
            candidates
                .into_iter()
                .map(|c| {
                    let value = c.get_value().to_string_lossy().to_string();
                    Pair { display: value.clone(), replacement: value }
                })
                .collect()
        };

        // 叠加 core 命令表 / 子命令 / 参数规格 / 自定义概念候选，按前缀去重。
        for extra in self.core_candidates(&committed, arg_index, &prefix) {
            if !pairs.iter().any(|p| p.replacement == extra) {
                pairs.push(Pair { display: extra.clone(), replacement: extra });
            }
        }

        let start = if trailing {
            pos
        } else if args.is_empty() {
            0
        } else {
            let last_arg = args.last().unwrap();
            let last_arg_str = last_arg.to_string();
            before_cursor.rfind(&last_arg_str as &str).unwrap_or(pos - last_arg_str.len())
        };

        Ok((start, pairs))
    }
}

impl ClapHelper {
    /// 首词命令名 / 子命令 / 参数位置的 core 表驱动候选。
    fn core_candidates(&self, committed: &[String], arg_index: usize, prefix: &str) -> Vec<String> {
        if committed.is_empty() {
            return filter_prefix(self.fetch_command_names(), prefix);
        }
        let parent = committed[0].clone();
        if committed.len() == 1 {
            let subs = self.fetch_subcommands(&parent);
            if !subs.is_empty() {
                return filter_prefix(subs, prefix);
            }
        }
        self.arg_candidates(committed, arg_index, prefix)
    }

    /// 按参数规格类别取内置来源候选；自定义概念回退插件 `complete_arg` 回调。
    fn arg_candidates(&self, committed: &[String], arg_index: usize, prefix: &str) -> Vec<String> {
        use cli_core_types::ui::ui_field_kind as k;
        let path = committed.join(" ");
        let (kind, custom) = self.fetch_arg_spec(&path, arg_index as i32);
        if !custom.is_empty() {
            return self.custom_candidates(committed, arg_index, prefix);
        }
        match kind {
            k::OPTION_KEY => {
                let needle = prefix.to_uppercase();
                self.live_options()
                    .iter()
                    .filter(|o| o.to_uppercase().starts_with(&needle))
                    .cloned()
                    .collect()
            }
            k::PLUGIN => {
                filter_prefix(self.live_plugins().iter().map(|(n, _)| n.clone()).collect(), prefix)
            }
            k::NAMESPACE => filter_prefix(
                self.live_plugins().iter().map(|(_, ns)| ns.clone()).collect(),
                prefix,
            ),
            k::LOCAL_PATH => local_path_candidates(prefix),
            k::REMOTE_PATH => self.remote_candidates(prefix),
            // REPL 无成员快照；MEMBER 暂不补全，回退 clap 基础候选。
            _ => Vec::new(),
        }
    }

    /// 实时拉取已加载插件 `(name, namespace)`；符号缺失时回退启动快照。
    fn live_plugins(&self) -> Vec<(String, String)> {
        let Some(list) = self.get_all_plugins else {
            return self.plugins.clone();
        };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = list(Out::from(&mut buf));
        ManuallyDrop::into_inner(buf)
            .iter()
            .map(|p| (p.name.to_string(), p.command_name.to_string()))
            .collect()
    }

    /// 实时拉取已注册选项 key；符号缺失时回退启动快照。
    fn live_options(&self) -> Vec<String> {
        let Some(list) = self.get_registered_options else {
            return self.options.clone();
        };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(Out::from(&mut buf)) };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    fn fetch_command_names(&self) -> Vec<String> {
        let Some(list) = self.list_commands else { return Vec::new() };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(Out::from(&mut buf)) };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    fn fetch_subcommands(&self, parent: &str) -> Vec<String> {
        let Some(list) = self.list_subcommands else { return Vec::new() };
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe { list(&parent.into(), Out::from(&mut buf)) };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    fn fetch_arg_spec(&self, path: &str, arg_index: i32) -> (i32, String) {
        use cli_core_types::ui::ui_field_kind as k;
        let Some(query) = self.arg_spec else { return (k::PLAIN, String::new()) };
        let mut spec: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        let _ = unsafe { query(&path.into(), &arg_index, Out::from(&mut spec)) };
        let row = ManuallyDrop::into_inner(spec).to_string();
        match row.split_once('|') {
            Some((kind, custom)) => (kind.trim().parse().unwrap_or(k::PLAIN), custom.to_string()),
            None => (k::NO_COMPLETION, String::new()),
        }
    }

    fn custom_candidates(
        &self,
        committed: &[String],
        arg_index: usize,
        prefix: &str,
    ) -> Vec<String> {
        let Some(complete) = self.complete_arg else { return Vec::new() };
        let argv: Vec<safer_ffi::String> = committed
            .iter()
            .cloned()
            .chain(std::iter::once(prefix.to_string()))
            .map(|s| s.into())
            .collect();
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _ = unsafe {
            complete(
                &safer_ffi::vec::Vec::from(argv),
                &(arg_index as i32),
                &prefix.into(),
                Out::from(&mut buf),
            )
        };
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    fn remote_candidates(&self, prefix: &str) -> Vec<String> {
        let Some(list_dir) = self.remote_list_dir else { return Vec::new() };
        let (parent, base) = split_path_prefix(prefix);
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
}

/// 按前缀过滤候选；前缀为空时返回全量。
fn filter_prefix(items: Vec<String>, prefix: &str) -> Vec<String> {
    if prefix.is_empty() {
        return items;
    }
    items.into_iter().filter(|s| s.starts_with(prefix)).collect()
}

/// 把路径前缀拆成 `(父目录, 基名)`，与 TUI 口径一致。
fn split_path_prefix(prefix: &str) -> (&str, &str) {
    match prefix.rfind('/') {
        Some(i) => (&prefix[..=i], &prefix[i + 1..]),
        None => ("", prefix),
    }
}

/// 列举本机目录条目，按基名前缀过滤；目录条目补 `/` 后缀。
fn local_path_candidates(prefix: &str) -> Vec<String> {
    let (parent, base) = split_path_prefix(prefix);
    let base_dir = if parent.is_empty() {
        std::env::current_dir().unwrap_or_default()
    } else {
        std::path::PathBuf::from(parent)
    };
    let Ok(rd) = std::fs::read_dir(&base_dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(base) {
            continue;
        }
        let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { "/" } else { "" };
        out.push(format!("{parent}{name}{suffix}"));
    }
    if base_dir.parent().is_some() && "..".starts_with(base) {
        out.push(format!("{parent}../"));
    }
    out
}

impl Hinter for ClapHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for ClapHelper {}
impl Validator for ClapHelper {}

fn simplify_branch(ref_name: Option<&str>) -> &str {
    ref_name.and_then(|r| r.strip_prefix("refs/heads/")).unwrap_or("no branch")
}

pub(crate) fn welcome_text(b: bool) -> String {
    let mut s = format!(
        "{} {}({}) ({}, {}, {})",
        built_info::PKG_NAME,
        built_info::PKG_VERSION,
        built_info::PROFILE,
        simplify_branch(built_info::GIT_HEAD_REF),
        built_info::GIT_COMMIT_HASH_SHORT.unwrap_or("unknown"),
        built_info::BUILT_TIME_UTC,
    );
    s.push_str(&format!("\n[{}] on {}", built_info::RUSTC_VERSION, built_info::CFG_OS));
    if b {
        s.push_str(&format!("\n{}", crate::tr("repl.welcome_tip")));
    }
    s
}

fn welcome(b: bool) {
    println!("{}", welcome_text(b));
}

pub(crate) fn build_info_text() -> String {
    format!(
        r"=== built::info ===
PKG_NAME: {}
PKG_VERSION: {}
TARGET: {}
HOST: {} ({})
PROFILE: {} (OPT_LEVEL: {})
RUSTC_VERSION: {}
BUILT_TIME_UTC: {}
GIT_BRANCH :{:?}
GIT_VERSION: {:?}
GIT_DIRTY: {:?}
GIT_COMMIT_HASH: {:?}
=== end of built::info ===",
        built_info::PKG_NAME,
        built_info::PKG_VERSION,
        built_info::TARGET,
        built_info::HOST,
        built_info::CFG_OS,
        built_info::PROFILE,
        built_info::OPT_LEVEL,
        built_info::RUSTC_VERSION,
        built_info::BUILT_TIME_UTC,
        simplify_branch(built_info::GIT_HEAD_REF),
        built_info::GIT_VERSION.unwrap_or("unknown"),
        built_info::GIT_DIRTY.unwrap_or(false),
        built_info::GIT_COMMIT_HASH.unwrap_or("unknown"),
    )
}

fn build_info() {
    println!("{}", build_info_text());
}

pub(crate) fn copyright_text() -> String {
    "Copyright (c) 2026 HFunction2013. All Rights Reserved.".to_string()
}

fn print_copyright() {
    println!("{}", copyright_text());
}

pub(crate) fn license_text() -> String {
    include_str!("../../../LICENSE").to_string()
}

fn print_license() {
    println!("{}", license_text());
}

/// 随机一句 fortune；供 REPL 与 TUI 内置命令复用。
pub(crate) fn saying_text() -> Option<String> {
    rs_fortune::choose_fortune(FORTUNE_TEXT).map(|s| s.to_string())
}

/// 内置 / 常用命令帮助文本；供 REPL 终端输出与 TUI 帮助弹窗复用。
pub(crate) fn builtin_help_lines() -> Vec<String> {
    let t = crate::tr;
    vec![
        t("help.line1").to_string(),
        t("help.line2").to_string(),
        t("help.line3").to_string(),
        t("help.line4").to_string(),
    ]
}

fn sha256_hex<T: AsRef<[u8]>>(data: T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_ref());
    format!("{:x}", hasher.finalize())
}

fn print_banner(colored: &String) {
    print!("{colored}");
}

fn colorize_string(cfg: &Config, input: &str) -> io::Result<String> {
    let mut output = Vec::new();
    let mut printer = Printer::new(cfg, true, choose_color_mode(cfg), initial_offset(cfg.seed));

    printer.print_text(input, &mut output)?;
    printer.finalize(&mut output)?;

    let colored =
        String::from_utf8(output).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(colored)
}

struct AtExit;
impl Drop for AtExit {
    fn drop(&mut self) {
        #[cfg(not(debug_assertions))]
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    }
}

fn prepare_startup() -> String {
    // TODO: stub tasks.
    let t = crate::tr;
    let tasks: Vec<(String, u64)> = vec![
        (t("start.loading_config").to_string(), 10),
        (t("start.initializing_modules").to_string(), 10),
        (t("start.connecting_server").to_string(), 10),
        (t("start.verifying_assets").to_string(), 20),
        (t("start.starting_services").to_string(), 10),
    ];

    let total_target: u64 = tasks.iter().map(|(_, t)| t).sum();

    let running = Arc::new(AtomicBool::new(true));
    let running_anim = running.clone();

    let progress = Arc::new(AtomicU64::new(0));
    let progress_pb = progress.clone();

    let pb = Arc::new(ProgressBar::new(total_target));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg:.bold} [{bar:20.cyan/blue}] {pos:.yellow}/{len:.yellow} ({elapsed_precise}) {percent:.bold}%")
            .unwrap()
            .progress_chars("█▓▒░"),
    );

    let colored = Arc::new(Mutex::new(String::new()));
    let colored_anim = colored.clone();
    let pb_anim = pb.clone();
    let pb_task = pb;

    let anim_handle = thread::spawn(move || {
        let Ok(font) = FIGlet::standard() else { return };
        let Some(art) = font.convert("Terminal Chat") else { return };
        let mut rng = rand::thread_rng();
        let mut i: u64 = rng.r#gen();
        let freq = 10;
        let note = String::from(crate::tr("start.starting_note"));
        let mut cfg = Config { speed: 4000.0, ..Default::default() };
        let mut display = note.clone();

        while running_anim.load(Ordering::SeqCst) {
            cfg.seed = i;
            pb_anim.suspend(|| {
                let _ = execute!(stdout(), MoveTo(0, 0));
                if let Ok(result) = colorize_string(&cfg, &art.as_str()) {
                    *colored_anim.lock().unwrap() = result.clone();
                    println!("{result}");
                }
            });

            pb_anim.set_position(progress_pb.load(Ordering::Relaxed));

            thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
            i = i.wrapping_add(1);

            if i.is_multiple_of(freq) {
                let pos = ((i / freq) as usize) % note.len();
                let mut chars: Vec<char> = note.chars().collect();
                if let Some(c) = chars.get_mut(pos)
                    && c.is_ascii_alphabetic()
                {
                    *c = if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    };
                }
                display = chars.into_iter().collect();
            }
            println!("{display}");
        }
    });

    let task_handle = thread::spawn(move || {
        for (name, target) in tasks {
            pb_task.set_message(name);

            // TODO: run task stub.
            for _ in 0..target {
                thread::sleep(Duration::from_millis(50));
                progress.fetch_add(1, Ordering::SeqCst);
                pb_task.inc(1);
            }
        }

        pb_task.finish_with_message(crate::tr("start.all_systems_ready"));
        running.store(false, Ordering::SeqCst);
    });

    task_handle.join().unwrap();
    anim_handle.join().unwrap();

    colored.lock().unwrap().clone()
}

/// 输出指定 shell 的补全脚本（bash/zsh/fish/powershell/elvish）。
fn print_completions(cli: &Command, shell_name: &str) -> Result<(), String> {
    use clap_complete::Shell;
    let shell = match shell_name.to_ascii_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "ps" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        "" => return Err("usage: tc-cli completion <bash|zsh|fish|powershell|elvish>".into()),
        other => {
            return Err(format!(
                "unsupported shell `{other}` (use bash/zsh/fish/powershell/elvish)"
            ));
        }
    };
    let mut cmd = cli.clone();
    clap_complete::generate(shell, &mut cmd, "tc-cli", &mut io::stdout());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 获取函数指针
    let run_command: Symbol<RunCommandFn> = unsafe { get_cli_core().get(b"run_command") }
        .map_err(|e| format!("Failed to find symbol 'run_command': {e}"))?;
    let get_all_plugins: Symbol<GetAllPluginsFn> =
        unsafe { get_cli_core().get(b"get_all_plugins") }
            .map_err(|e| format!("Failed to find symbol 'get_all_plugins': {e}"))?;
    let load_plugin: Symbol<LoadPluginFn> = unsafe { get_cli_core().get(b"load_plugin") }
        .map_err(|e| format!("Failed to find symbol 'load_plugin': {e}"))?;
    // 默认加载 cli-standard；失败时明确报错而不是静默忽略。
    let load_res = load_plugin(&"cli_standard".into());
    if load_res.success != 1 {
        eprintln!(
            "Failed to load cli_standard: exit_code={} msg={:?}",
            load_res.exit_code, load_res.msg
        );
    }

    #[cfg(unix)]
    unsafe {
        signal(SIGTSTP, libc::SIG_IGN);
    }
    install_ctrlc_handler();
    let _guard = AtExit;

    // Stub now. will make cli-core command part a module named std.
    // let cli = yaml2cmd::add_commands_from_yaml(include_str!("../../cli-core/commands.yaml"));
    let mut plugin_buf: ManuallyDrop<safer_ffi::vec::Vec<PluginMetadata>> =
        ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
    let _res = get_all_plugins(Out::from(&mut plugin_buf));
    let plugins: Vec<PluginMetadata> = ManuallyDrop::into_inner(plugin_buf).into();
    let mut cli = Command::new("tc-cli").no_binary_name(true);
    for plugin in &plugins {
        let command_yaml: &safer_ffi::String = &plugin.command_yaml;
        cli = yaml2cmd::add_commands_from_yaml(command_yaml, &cli);
    }

    // `tc-cli completion <shell>`：输出 shell 补全脚本（供 bash/zsh/fish 等 source）。
    // 放在 banner 之前，保证输出纯补全脚本、不被启动画面污染。
    {
        let argv: Vec<String> = std::env::args().collect();
        if argv.get(1).map(|s| s.as_str()) == Some("completion") {
            let name = argv.get(2).map(|s| s.as_str()).unwrap_or("");
            match print_completions(&cli, name) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
    }

    // `tc-cli --tui`：edit-tui 框架的全屏聊天界面。
    if std::env::args().any(|a| a == "--tui") {
        let core_lib = get_cli_core();
        let take_pushes: Symbol<PushDrainFn> = unsafe { core_lib.get(b"tc_cli_push_drain") }
            .map_err(|e| format!("Failed to find symbol 'tc_cli_push_drain': {e}"))?;
        let get_option: Symbol<tui::GetOptionFn> = unsafe { core_lib.get(b"get_global_option") }
            .map_err(|e| format!("Failed to find symbol 'get_global_option': {e}"))?;
        // 会话选项为可选符号：缺失时偏好读取仅回退全局。
        let get_session: Option<Symbol<tui::GetOptionFn>> =
            unsafe { core_lib.get(b"get_session_option") }.ok();
        let get_session_ref: Option<&tui::GetOptionFn> = get_session.as_deref();
        let set_tui_mode: Symbol<tui::SetTuiModeFn> =
            unsafe { core_lib.get(b"tc_cli_set_tui_mode") }
                .map_err(|e| format!("Failed to find symbol 'tc_cli_set_tui_mode': {e}"))?;
        let output_drain: Symbol<tui::OutputDrainFn> =
            unsafe { core_lib.get(b"tc_cli_output_drain") }
                .map_err(|e| format!("Failed to find symbol 'tc_cli_output_drain': {e}"))?;
        // 远端目录列举为可选符号：旧 core 缺失时 TUI 远端路径不补全，不阻断启动。
        let remote_list_dir: Option<Symbol<ListRemoteDirFn>> =
            unsafe { core_lib.get(b"remote_list_dir") }.ok();
        let remote_list_dir_ref: Option<&ListRemoteDirFn> = remote_list_dir.as_deref();
        // 连接态查询为可选符号：缺失时 TUI 视为未连接。
        let is_connected: Option<Symbol<IsConnectedFn>> =
            unsafe { core_lib.get(b"is_connected") }.ok();
        let is_connected_ref: Option<&IsConnectedFn> = is_connected.as_deref();
        // 已注册选项列举为可选符号：缺失时选项 key 不补全。
        let registered_options: Option<Symbol<tui::GetRegisteredOptionsFn>> =
            unsafe { core_lib.get(b"get_registered_options") }.ok();
        let registered_options_ref: Option<&tui::GetRegisteredOptionsFn> =
            registered_options.as_deref();
        // 命令表 / 子命令 / 参数规格 / 自定义概念补全转发为可选符号：缺失时对应补全退化。
        let list_commands: Option<Symbol<tui::ListCommandsFn>> =
            unsafe { core_lib.get(b"list_commands") }.ok();
        let list_commands_ref: Option<&tui::ListCommandsFn> = list_commands.as_deref();
        let list_subcommands: Option<Symbol<tui::ListSubcommandsFn>> =
            unsafe { core_lib.get(b"list_subcommands") }.ok();
        let list_subcommands_ref: Option<&tui::ListSubcommandsFn> = list_subcommands.as_deref();
        let arg_spec: Option<Symbol<tui::ArgSpecFn>> = unsafe { core_lib.get(b"arg_spec") }.ok();
        let arg_spec_ref: Option<&tui::ArgSpecFn> = arg_spec.as_deref();
        let complete_arg: Option<Symbol<tui::CompleteArgHostFn>> =
            unsafe { core_lib.get(b"complete_arg") }.ok();
        let complete_arg_ref: Option<&tui::CompleteArgHostFn> = complete_arg.as_deref();
        // 命令中断标志为可选符号：缺失时 TUI Ctrl+C 不中断命令。
        let set_interrupted: Option<Symbol<SetInterruptedFn>> =
            unsafe { core_lib.get(b"set_interrupted") }.ok();
        let set_interrupted_ref: Option<&SetInterruptedFn> = set_interrupted.as_deref();
        let set_current_freq: Option<Symbol<SetCurrentFreqFn>> =
            unsafe { core_lib.get(b"set_current_freq") }.ok();
        let set_current_freq_ref: Option<&SetCurrentFreqFn> = set_current_freq.as_deref();
        // 终端占用查询为可选符号：缺失时视为空闲，TUI 照常渲染。
        let terminal_busy: Option<Symbol<tui::TerminalBusyFn>> =
            unsafe { core_lib.get(b"tc_cli_terminal_is_busy") }.ok();
        let terminal_busy_ref: Option<&tui::TerminalBusyFn> = terminal_busy.as_deref();
        // 注册表菜单 / 表单 / 侧栏 / 底部查询为可选符号：缺失或为空时对应条目为空，
        // 宿主仅保留本地内置命令菜单（File→Exit、Help 组），不再回退整套硬编码 UI。
        let get_menu_items: Option<Symbol<ui_registry::GetMenuItemsFn>> =
            unsafe { core_lib.get(b"get_menu_items") }.ok();
        let get_menu_items_ref: Option<&ui_registry::GetMenuItemsFn> = get_menu_items.as_deref();
        let get_form_spec: Option<Symbol<ui_registry::GetFormSpecFn>> =
            unsafe { core_lib.get(b"get_form_spec") }.ok();
        let get_form_spec_ref: Option<&ui_registry::GetFormSpecFn> = get_form_spec.as_deref();
        let get_bottom_fragments: Option<Symbol<ui_registry::GetBottomFragmentsFn>> =
            unsafe { core_lib.get(b"get_bottom_fragments") }.ok();
        let get_bottom_fragments_ref: Option<&ui_registry::GetBottomFragmentsFn> =
            get_bottom_fragments.as_deref();
        let get_side_sections: Option<Symbol<ui_registry::GetSideSectionsFn>> =
            unsafe { core_lib.get(b"get_side_sections") }.ok();
        let get_side_sections_ref: Option<&ui_registry::GetSideSectionsFn> =
            get_side_sections.as_deref();
        let get_side_items: Option<Symbol<ui_registry::GetSideItemsFn>> =
            unsafe { core_lib.get(b"get_side_items") }.ok();
        let get_side_items_ref: Option<&ui_registry::GetSideItemsFn> = get_side_items.as_deref();
        register_terminal_hooks(
            tui_terminal_suspend as *const (),
            tui_terminal_resume as *const (),
        );
        return tui::run(
            &run_command,
            &take_pushes,
            &get_option,
            get_session_ref,
            &set_tui_mode,
            &output_drain,
            remote_list_dir_ref,
            is_connected_ref,
            registered_options_ref,
            list_commands_ref,
            list_subcommands_ref,
            arg_spec_ref,
            complete_arg_ref,
            set_interrupted_ref,
            set_current_freq_ref,
            terminal_busy_ref,
            get_menu_items_ref,
            get_form_spec_ref,
            get_bottom_fragments_ref,
            get_side_sections_ref,
            get_side_items_ref,
        )
        .map_err(|e| e.into());
    }

    #[cfg(not(debug_assertions))]
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), Hide)?;
    register_terminal_hooks(repl_terminal_suspend as *const (), repl_terminal_resume as *const ());
    let colored = prepare_startup();
    execute!(stdout(), Show)?;

    // 拉取已注册选项 key 快照（符号缺失时的回退）；同时保留符号供补全时实时拉取。
    let registered_options_sym: Option<Symbol<tui::GetRegisteredOptionsFn>> =
        unsafe { get_cli_core().get(b"get_registered_options") }.ok();
    let registered_options: Vec<String> = registered_options_sym
        .as_deref()
        .map(|sym| {
            let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
                ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
            let _ = unsafe { sym(Out::from(&mut buf)) };
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
        })
        .unwrap_or_default();

    // 命令表 / 子命令 / 参数规格 / 自定义概念补全转发为可选符号：缺失时对应补全退化。
    let list_commands: Option<Symbol<tui::ListCommandsFn>> =
        unsafe { get_cli_core().get(b"list_commands") }.ok();
    let list_subcommands: Option<Symbol<tui::ListSubcommandsFn>> =
        unsafe { get_cli_core().get(b"list_subcommands") }.ok();
    let arg_spec: Option<Symbol<tui::ArgSpecFn>> = unsafe { get_cli_core().get(b"arg_spec") }.ok();
    let complete_arg: Option<Symbol<tui::CompleteArgHostFn>> =
        unsafe { get_cli_core().get(b"complete_arg") }.ok();
    let remote_list_dir: Option<Symbol<ListRemoteDirFn>> =
        unsafe { get_cli_core().get(b"remote_list_dir") }.ok();

    // 插件快照 `(name, command_namespace)`，供插件名 / 命名空间参数位置补全。
    let plugin_snapshot: Vec<(String, String)> =
        plugins.iter().map(|p| (p.name.to_string(), p.command_name.to_string())).collect();

    let helper = ClapHelper {
        cli: cli.clone(),
        options: registered_options,
        plugins: plugin_snapshot,
        list_commands: list_commands.as_deref().copied(),
        list_subcommands: list_subcommands.as_deref().copied(),
        arg_spec: arg_spec.as_deref().copied(),
        complete_arg: complete_arg.as_deref().copied(),
        remote_list_dir: remote_list_dir.as_deref().copied(),
        get_all_plugins: Some(*get_all_plugins),
        get_registered_options: registered_options_sym.as_deref().copied(),
    };
    let mut rl = Editor::<ClapHelper, _>::new()?;
    rl.set_helper(Some(helper));
    // 异步推送显示：tty 下用 rustyline 的 ExternalPrinter（在输入行上方打印，
    // 保留已输入内容、不打断输入）；管道/重定向下退化到 stdout 直接打印。
    // 队列在宿主 cli_core（单实例），由 cli_core 的 tc_cli_push_drain 取走。
    let core_lib = get_cli_core();
    let take_pushes: Symbol<PushDrainFn> = unsafe { core_lib.get(b"tc_cli_push_drain") }
        .map_err(|e| format!("Failed to find symbol 'tc_cli_push_drain': {e}"))?;
    // 可选：事件唤醒等待；旧核心无此符号时退化为固定间隔轮询。
    let push_wait: Option<PushWaitFn> =
        unsafe { core_lib.get(b"tc_cli_push_wait").ok().map(|sym| *sym) };
    if io::stdout().is_terminal() {
        let mut printer = rl.create_external_printer()?;
        thread::spawn(move || {
            loop {
                match push_wait {
                    Some(wait) => {
                        let _ = unsafe { wait(200) };
                    }
                    None => thread::sleep(Duration::from_millis(150)),
                }
                let texts: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
                for t in texts {
                    let _ = printer.print(t.to_string());
                }
            }
        });
    } else {
        thread::spawn(move || {
            loop {
                match push_wait {
                    Some(wait) => {
                        let _ = unsafe { wait(200) };
                    }
                    None => thread::sleep(Duration::from_millis(150)),
                }
                let texts: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
                for t in texts {
                    println!("{t}");
                }
            }
        });
    }
    rl.clear_screen()?;
    print_banner(&colored);
    welcome(true);

    // 后台命令 worker：命令在独立线程串行执行，主线程（readline）不被阻塞命令卡住；
    // 命令输出由插件经 `print_content` 直接写终端，错误结果在此回打。单命令串行槽位，
    // 避免并发 FFI / 连接竞争。
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Vec<String>>();
    let cmd_running = Arc::new(AtomicBool::new(false));
    {
        let cmd_running_w = cmd_running.clone();
        let run_cmd_fn: RunCommandFn = *run_command;
        thread::spawn(move || {
            while let Ok(argv) = cmd_rx.recv() {
                cmd_running_w.store(true, Ordering::SeqCst);
                let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
                    argv.iter().map(|s| s.as_str().into()).collect::<Vec<_>>().into();
                let result = run_cmd_fn(&args_ffi);
                if result.code != 0 {
                    eprintln!("{}", result.message);
                }
                cmd_running_w.store(false, Ordering::SeqCst);
            }
        });
    }

    loop {
        // 检查到期的本地调度任务（schedule / schedule_msg）
        let _ = tasks::check_and_run(&run_command);
        // 通知不再每轮轮询：由后台推送线程异步接收并显示在输入行上方。
        let input = match rl.readline("tc> ") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", crate::tr("repl.bye"));
                break;
            }
            Err(err) => {
                eprintln!("Readline error: {err}");
                break;
            }
        };

        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if sha256_hex(input).as_str()
            == "6ac3c336e4094835293a3fed8a4b5fedde1b5e2626d9838fed50693bba00af0e"
        {
            println!(
                "{}",
                String::from_utf8_lossy(
                    &general_purpose::STANDARD.decode("ZnVjayB5b3UgdG9vLCBidWRkeS4=").unwrap()
                )
            );
            continue;
        }
        rl.add_history_entry(input)?;
        // TTY 下 rustyline 已在终端留下 `tc> <输入>` 行，不再重复回显；
        // 管道 / 重定向下 rustyline 不渲染提示符，补打一行保持口径一致。
        if !io::stdout().is_terminal() {
            println!("{}", repl_echo_line(input));
        }
        match input {
            "exit" | "quit" | "q" => {
                println!("{}", crate::tr("repl.bye"));
                break;
            }
            "clear" => {
                let _ = rl.clear_screen();
                continue;
            }
            "copyright" => {
                print_copyright();
                continue;
            }
            "license" => {
                print_license();
                continue;
            }
            "version" => {
                welcome(false);
                continue;
            }
            "build_info" => {
                build_info();
                continue;
            }
            "about" => {
                println!("{}", welcome_text(false));
                println!();
                println!("{}", build_info_text());
                continue;
            }
            "help" => {
                for line in builtin_help_lines() {
                    println!("{line}");
                }
                continue;
            }
            "coffee" => {
                println!(
                    r"                  ( (
                   ) )
                 ........
                 |      |]
                 \      /
                  `----'
Because everyone deserves a good cup of coffee."
                );
                continue;
            }
            "banner" => {
                print_banner(&colored);
                continue;
            }
            "train" => {
                let _ = sl::run_sl();
                rl.clear_screen()?;
                continue;
            }
            "saying" => {
                match saying_text() {
                    Some(msg) => println!("{msg}"),
                    None => eprintln!("{}", crate::tr("repl.saying_error")),
                }
                continue;
            }
            _ => {}
        }

        let args = match shell_words::split(input) {
            Ok(args) => args,
            Err(e) => {
                eprintln!("Parse error: {e}");
                continue;
            }
        };

        // 投递到后台 worker 线程执行，不阻塞 readline；单命令串行槽位，
        // 上一条命令未跑完则提示等待，避免并发 FFI / 连接竞争。
        if cmd_running.load(Ordering::SeqCst) {
            eprintln!("{}", crate::tr("repl.prev_still_running"));
            continue;
        }
        if cmd_tx.send(args).is_err() {
            eprintln!("{}", crate::tr("repl.worker_unavailable"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i18n_lookup_en_zh_and_fallback() {
        use crate::i18n::{Lang, t_lang};
        assert_eq!(t_lang(Lang::En, "menu.file"), "File");
        assert_eq!(t_lang(Lang::Zh, "menu.file"), "文件");
        // Unknown key echoes the key back verbatim.
        assert_eq!(t_lang(Lang::En, "no.such.key"), "no.such.key");
        assert_eq!(t_lang(Lang::Zh, "no.such.key"), "no.such.key");
    }

    static STUB_CMDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static STUB_SPEC: Mutex<Vec<(String, i32, i32, String)>> = Mutex::new(Vec::new());
    static STUB_CUSTOM: Mutex<Vec<String>> = Mutex::new(Vec::new());

    unsafe extern "C" fn stub_list_commands(
        out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
    ) -> cli_core_types::Result {
        let rows: Vec<safer_ffi::String> =
            STUB_CMDS.lock().unwrap().iter().map(|s| s.clone().into()).collect();
        out.write(rows.into());
        cli_core_types::Result::success()
    }
    const STUB_LIST_COMMANDS: tui::ListCommandsFn = stub_list_commands;

    unsafe extern "C" fn stub_arg_spec(
        path: &safer_ffi::String,
        arg_index: &i32,
        out: Out<'_, safer_ffi::String>,
    ) -> cli_core_types::Result {
        let table = STUB_SPEC.lock().unwrap().clone();
        let parts: Vec<&str> = path.split_whitespace().collect();
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
    const STUB_ARG_SPEC: tui::ArgSpecFn = stub_arg_spec;

    unsafe extern "C" fn stub_complete_arg(
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
    const STUB_COMPLETE_ARG: tui::CompleteArgHostFn = stub_complete_arg;

    fn stub_helper() -> ClapHelper {
        ClapHelper {
            cli: Command::new("tc-cli").no_binary_name(true),
            options: Vec::new(),
            plugins: Vec::new(),
            list_commands: Some(STUB_LIST_COMMANDS),
            list_subcommands: None,
            arg_spec: Some(STUB_ARG_SPEC),
            complete_arg: Some(STUB_COMPLETE_ARG),
            remote_list_dir: None,
            get_all_plugins: None,
            get_registered_options: None,
        }
    }

    #[test]
    fn repl_command_names_come_from_core_table() {
        *STUB_CMDS.lock().unwrap() = vec!["msg".into(), "ls".into()];
        let h = stub_helper();
        assert_eq!(h.core_candidates(&[], 0, "m"), vec!["msg".to_string()]);
        assert_eq!(h.core_candidates(&[], 0, ""), vec!["msg", "ls"]);
    }

    #[test]
    fn repl_plugin_arg_uses_plugin_snapshot() {
        *STUB_SPEC.lock().unwrap() = vec![("plugin load".into(), 1, 6, String::new())];
        let mut h = stub_helper();
        h.plugins = vec![("cli-standard".into(), "std".into())];
        assert_eq!(
            h.core_candidates(&["plugin".to_string(), "load".to_string()], 2, "cli"),
            vec!["cli-standard".to_string()]
        );
    }

    #[test]
    fn repl_custom_arg_routes_to_plugin_callback() {
        *STUB_SPEC.lock().unwrap() = vec![("do thing".into(), 1, 0, "USER".into())];
        *STUB_CUSTOM.lock().unwrap() = vec!["alice".into(), "bob".into()];
        let h = stub_helper();
        assert_eq!(
            h.core_candidates(&["do".to_string(), "thing".to_string()], 2, "a"),
            vec!["alice".to_string()]
        );
    }

    #[test]
    fn option_candidates_match_prefix_case_insensitive() {
        let opts = vec!["SERVER".into(), "PORT".into(), "PREFER_COMMAND_LINE".into()];
        let c = option_candidates(&opts, Some("set"), 1, "pre");
        assert_eq!(c, vec!["PREFER_COMMAND_LINE".to_string()]);
    }

    #[test]
    fn option_candidates_empty_wrong_command_or_position() {
        let opts = vec!["SERVER".into()];
        // 非选项命令不补全选项 key。
        assert!(option_candidates(&opts, Some("msg"), 1, "se").is_empty());
        // 第 2 参（值位置）不补全选项 key。
        assert!(option_candidates(&opts, Some("set"), 2, "se").is_empty());
        // 无命令首词时不补全。
        assert!(option_candidates(&opts, None, 1, "se").is_empty());
    }

    #[test]
    fn repl_echo_line_prefixes_command() {
        assert_eq!(repl_echo_line("unload plugin cli-standard"), "tc> unload plugin cli-standard");
    }
}
