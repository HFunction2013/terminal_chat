use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use clap::Command;
use clap_complete::engine::complete;
use cli_core_types::Result as RunCommandResult;
use cli_core_types::{PluginMetadata, PluginResult};
#[cfg(not(debug_assertions))]
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
};
use figlet_rs::FIGlet;
use indicatif::{ProgressBar, ProgressStyle};
use libc::{SIGTSTP, signal};
use libloading::{Library, Symbol, library_filename};
use lolcat::{Config, Printer, choose_color_mode, initial_offset};
use rand::Rng;
use rustyline::{
    Context, Editor, Helper,
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
    io::{self, Write, stdout},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
mod fortune;
const FORTUNE_TEXT: &str = include_str!("../fortune-people.txt");
static CLI_CORE: OnceLock<Library> = OnceLock::new();
fn get_cli_core() -> &'static Library {
    let lib_path = get_library_path();

    CLI_CORE.get_or_init(|| unsafe {
        Library::new(&lib_path)
            .unwrap_or_else(|e| panic!("Failed to load library '{}': {}", lib_path.display(), e))
    })
}
fn get_library_path() -> PathBuf {
    let lib_name_buf = library_filename("cli_core");
    let lib_name = lib_name_buf.to_string_lossy();

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let path = dir.join(lib_name.as_ref());
        if path.exists() {
            return path;
        }
    }

    let cwd_path = PathBuf::from(lib_name.as_ref());
    if cwd_path.exists() {
        return cwd_path;
    }

    PathBuf::from(lib_name.as_ref())
}

fn install_ctrlc_handler() {
    // type IsInterruptedFn = unsafe extern "C" fn() -> bool;
    type IsInCmdFn = unsafe extern "C" fn() -> bool;
    type SetInterruptedFn = unsafe extern "C" fn(bool);

    let lib = get_cli_core();
    // 立即解引用为原始函数指针，不保留 Symbol
    // let is_interrupted: IsInterruptedFn = *unsafe { lib.get(b"is_interrupted") }.expect("Failed to find symbol 'is_interrupted'");

    let is_in_cmd: IsInCmdFn =
        *unsafe { lib.get(b"is_in_cmd") }.expect("Failed to find symbol 'is_in_cmd'");
    let set_interrupted: SetInterruptedFn =
        *unsafe { lib.get(b"set_interrupted") }.expect("Failed to find symbol 'set_interrupted'");

    ctrlc::set_handler(move || unsafe {
        set_interrupted(true);
        if !is_in_cmd() {
            #[cfg(not(debug_assertions))]
            let _ = execute!(stdout(), LeaveAlternateScreen);
            let _ = execute!(stdout(), Show);
            std::process::exit(0);
        }
        let _ = writeln!(std::io::stderr(), " (interrupt sent to current command)");
    })
    .expect("failed to install Ctrl+C handler");
}

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// rustyline Helper：桥接 `clap_complete`
struct ClapHelper {
    cli: Command,
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

        let arg_index = args.len();

        let args_os: Vec<OsString> =
            std::iter::once("tc-cli".to_string()).chain(args).map(OsString::from).collect();

        let mut cli = self.cli.clone();

        let candidates =
            complete(&mut cli, args_os, arg_index, Some(Path::new("."))).unwrap_or_default();

        let pairs = candidates
            .into_iter()
            .map(|c| {
                let value = c.get_value().to_string_lossy().to_string();
                Pair { display: value.clone(), replacement: value }
            })
            .collect();

        let start = line[..pos].rfind(char::is_whitespace).map_or(0, |i| i + 1);

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

fn welcome(b: bool) {
    println!(
        "{} {}({}) ({}, {}, {})",
        built_info::PKG_NAME,
        built_info::PKG_VERSION,
        built_info::PROFILE,
        simplify_branch(built_info::GIT_HEAD_REF),
        built_info::GIT_COMMIT_HASH_SHORT.unwrap_or("unknown"),
        built_info::BUILT_TIME_UTC,
    );
    println!("[{}] on {}", built_info::RUSTC_VERSION, built_info::CFG_OS);
    if b {
        println!("Type \"help\", \"copyright\" or \"license\" for more information.");
    }
}

fn build_info() {
    let out = format!(
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
=== end of built::info ===
",
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
    );

    let _ = io::stdout().write_all(out.as_bytes());
    let _ = io::stdout().flush();
}

fn print_copyright() {
    println!("Copyright (c) 2026 HZFY. All Rights Reserved.");
}

fn print_license() {
    let _ = io::stdout().write_all(include_bytes!("../../../LICENSE"));
    println!();
    let _ = io::stdout().flush();
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
    let running = Arc::new(AtomicBool::new(true));
    let running_anim = running.clone();

    // TODO: stub tasks.
    let tasks = vec![
        ("Loading config", 10),
        ("Initializing modules", 10),
        ("Connecting to server", 10),
        ("Verifying assets", 20),
        ("Starting services", 10),
    ];

    let total_target: u64 = tasks.iter().map(|(_, t)| t).sum();

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
        let mut rng = rand::thread_rng();
        let mut i: u64 = rng.r#gen();
        let freq = 10;
        let note = String::from("Terminal Chat Starting");
        let font = FIGlet::standard().unwrap();
        let art = font.convert("Terminal Chat").unwrap();
        let mut cfg = Config { speed: 4000.0, ..Default::default() };
        let mut display = note.clone();

        while running_anim.load(Ordering::SeqCst) {
            cfg.seed = i;
            pb_anim.suspend(|| {
                execute!(stdout(), MoveTo(0, 0)).unwrap();
                let result = colorize_string(&cfg, &art.as_str()).unwrap();
                *colored_anim.lock().unwrap() = result.clone();
                println!("{result}");
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

        pb_task.finish_with_message("All systems ready!");
        running.store(false, Ordering::SeqCst);
    });

    task_handle.join().unwrap();
    anim_handle.join().unwrap();

    colored.lock().unwrap().clone()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 获取 run_command 函数指针
    type RunCommandFn =
        unsafe extern "C" fn(&safer_ffi::vec::Vec<safer_ffi::String>) -> RunCommandResult;
    type GetAllPluginsFn = unsafe extern "C" fn() -> safer_ffi::Vec<PluginMetadata>;
    type LoadPluginFn = unsafe extern "C" fn(&safer_ffi::String) -> PluginResult;

    let run_command: Symbol<RunCommandFn> = unsafe { get_cli_core().get(b"run_command") }
        .map_err(|e| format!("Failed to find symbol 'run_command': {e}"))?;
    let get_all_plugins: Symbol<GetAllPluginsFn> =
        unsafe { get_cli_core().get(b"get_all_plugins") }
            .map_err(|e| format!("Failed to find symbol 'get_all_plugins': {e}"))?;
    let load_plugin: Symbol<LoadPluginFn> = unsafe { get_cli_core().get(b"load_plugin") }
        .map_err(|e| format!("Failed to find symbol 'load_plugin': {e}"))?;
    let _ =
        unsafe { load_plugin(&library_filename("cli_standard").to_string_lossy().as_ref().into()) };

    unsafe {
        signal(SIGTSTP, libc::SIG_IGN);
    }
    install_ctrlc_handler();
    let _guard = AtExit;
    #[cfg(not(debug_assertions))]
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), Hide)?;
    let colored = prepare_startup();
    execute!(stdout(), Show)?;

    // Stub now. will make cli-core command part a module named std.
    // let cli = yaml2cmd::add_commands_from_yaml(include_str!("../../cli-core/commands.yaml"));
    let plugins: Vec<PluginMetadata> = unsafe { get_all_plugins() }.into();
    let mut cli = Command::new("tc-cli").no_binary_name(true);
    for plugin in &plugins {
        let command_yaml: &safer_ffi::String = &plugin.command_yaml;
        cli = yaml2cmd::add_commands_from_yaml(command_yaml, &cli);
    }

    let helper = ClapHelper { cli: cli.clone() };
    let mut rl = Editor::<ClapHelper, _>::new()?;
    rl.set_helper(Some(helper));
    rl.clear_screen()?;
    print_banner(&colored);
    welcome(true);
    loop {
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
                match fortune::choose_fortune(FORTUNE_TEXT) {
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
        let result = unsafe { run_command(&args_ffi) };
        if result.code != 0 {
            eprintln!("{}", result.message);
        }
    }

    Ok(())
}
