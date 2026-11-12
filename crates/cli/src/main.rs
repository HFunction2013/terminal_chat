use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use clap::Command;
use clap_complete::engine::complete;
use cli_core_types::{
    GetAllPluginsFn, IsConnectedFn, IsInCmdFn, LoadPluginFn, PluginMetadata, RunCommandFn,
    SetInterruptedFn, plugin_fn_types::ListRemoteDirFn,
};
#[cfg(not(debug_assertions))]
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{
    cursor::{Hide, Show},
    execute,
};
use figlet_rs::FIGlet;
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(unix)]
use libc::{SIGTSTP, signal};
use libloading::{Library, Symbol};
use lolcat::{Config, Printer, choose_color_mode, initial_offset};
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

/// cli-core 导出：取走异步推送队列（safer_ffi Vec<String>，C ABI）。
type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::Vec<safer_ffi::String>;

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
        let _ = eprintln!(" (interrupt sent to current command)");
    })
    .expect("failed to install Ctrl+C handler");
}

mod tasks;
mod tui;
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// 选项命令在 key 位置的补全候选：仅当首词为选项命令且处于第 1 参（key）位置时，
/// 按大小写不敏感前缀过滤已注册选项。
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

/// rustyline Helper：桥接 `clap_complete`
struct ClapHelper {
    cli: Command,
    /// 已注册选项 key（供 set/setg/get/... 的参数位置补全）。
    options: Vec<String>,
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

        let is_starting_new_arg = before_cursor.ends_with(' ');
        let arg_index = if is_starting_new_arg { args.len() + 1 } else { args.len() };

        let args_os: Vec<OsString> =
            std::iter::once("tc-cli".to_string()).chain(args.clone()).map(OsString::from).collect();

        let mut cli = self.cli.clone();

        let candidates =
            complete(&mut cli, args_os, arg_index, Some(Path::new("."))).unwrap_or_default();

        let mut pairs: Vec<Pair> = candidates
            .into_iter()
            .map(|c| {
                let value = c.get_value().to_string_lossy().to_string();
                Pair { display: value.clone(), replacement: value }
            })
            .collect();

        // 选项命令的 key 位置：叠加已注册选项候选（大小写不敏感前缀匹配）。
        let first = args.first().map(|s| s.to_string());
        let prefix = args.last().map(|s| s.to_string()).unwrap_or_default();
        for opt in option_candidates(&self.options, first.as_deref(), arg_index, &prefix) {
            if !pairs.iter().any(|p| p.replacement == opt) {
                pairs.push(Pair { display: opt.clone(), replacement: opt });
            }
        }

        let start = if is_starting_new_arg {
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
        s.push_str("\nType \"help\", \"copyright\" or \"license\" for more information.");
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
    "Copyright (c) 2026 HZFY. All Rights Reserved.".to_string()
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
    let tasks = vec![
        ("Loading config", 10),
        ("Initializing modules", 10),
        ("Connecting to server", 10),
        ("Verifying assets", 20),
        ("Starting services", 10),
    ];

    let total_target: u64 = tasks.iter().map(|(_, t)| t).sum();

    let pb = Arc::new(ProgressBar::new(total_target));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg:.bold} [{bar:20.cyan/blue}] {pos:.yellow}/{len:.yellow} ({elapsed_precise}) {percent:.bold}%")
            .unwrap()
            .progress_chars("█▓▒░"),
    );

    // 后台只计算一次彩色 figlet art 写入内存；启动期间不向终端打印，
    // 避免与进度条并发写同一 stdout 造成行/光标错乱。
    let colored = Arc::new(Mutex::new(String::new()));
    let colored_bg = colored.clone();
    let art_handle = thread::spawn(move || {
        let font = match FIGlet::standard() {
            Ok(font) => font,
            Err(_) => return,
        };
        let Some(art) = font.convert("Terminal Chat") else {
            return;
        };
        let cfg = Config { speed: 4000.0, ..Default::default() };
        if let Ok(result) = colorize_string(&cfg, &art.as_str()) {
            *colored_bg.lock().unwrap() = result;
        }
    });

    for (name, target) in tasks {
        pb.set_message(name);
        // TODO: run task stub.
        for _ in 0..target {
            thread::sleep(Duration::from_millis(50));
            pb.inc(1);
        }
    }

    pb.finish_with_message("All systems ready!");

    art_handle.join().unwrap();
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
        )
        .map_err(|e| e.into());
    }

    #[cfg(not(debug_assertions))]
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), Hide)?;
    let colored = prepare_startup();
    execute!(stdout(), Show)?;

    // 拉取已注册选项 key，供 REPL 选项命令的参数位置补全；符号缺失时退化为空候选。
    let registered_options: Vec<String> = unsafe { get_cli_core().get(b"get_registered_options") }
        .ok()
        .map(|sym: Symbol<tui::GetRegisteredOptionsFn>| {
            let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
                ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
            let _ = unsafe { sym(Out::from(&mut buf)) };
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
        })
        .unwrap_or_default();

    let helper = ClapHelper { cli: cli.clone(), options: registered_options };
    let mut rl = Editor::<ClapHelper, _>::new()?;
    rl.set_helper(Some(helper));
    // 异步推送显示：tty 下用 rustyline 的 ExternalPrinter（在输入行上方打印，
    // 保留已输入内容、不打断输入）；管道/重定向下退化到 stdout 直接打印。
    // 队列在宿主 cli_core（单实例），由 cli_core 的 tc_cli_push_drain 取走。
    let core_lib = get_cli_core();
    let take_pushes: Symbol<PushDrainFn> = unsafe { core_lib.get(b"tc_cli_push_drain") }
        .map_err(|e| format!("Failed to find symbol 'tc_cli_push_drain': {e}"))?;
    if io::stdout().is_terminal() {
        let mut printer = rl.create_external_printer()?;
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(150));
                let texts: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
                for t in texts {
                    let _ = printer.print(t.to_string());
                }
            }
        });
    } else {
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(150));
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
                println!("Bye");
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
        println!("{}", repl_echo_line(input));
        match input {
            "exit" | "quit" | "q" => {
                println!("Bye");
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
                    None => eprintln!("Error choosing saying"),
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

        // 转换为 FFI 兼容的类型
        let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
            args.into_iter().map(|s| s.into()).collect::<Vec<_>>().into();

        // 通过动态库调用 run_command
        let result = run_command(&args_ffi);
        if result.code != 0 {
            eprintln!("{}", result.message);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{option_candidates, repl_echo_line};

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
