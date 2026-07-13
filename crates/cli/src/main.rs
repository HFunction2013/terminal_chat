use anyhow::Result;
use clap::ArgMatches;
use clap::Command;
use clap_complete::engine::complete;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use shell_words::split;
use std::ffi::OsString;
use std::path::Path;
mod commands;
use commands::all_commands;
use std::io::Write;
use std::io::{self};
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
mod command {
    include!(concat!(env!("OUT_DIR"), "/command.rs"));
}
/// rustyline Helper：桥接 clap_complete
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

        // 当前正在补全的参数索引
        let arg_index = args.len();

        // ✅ clap_complete 要求 Vec<OsString>
        let args_os: Vec<OsString> = std::iter::once("tc-cli".to_string())
            .chain(args)
            .map(OsString::from)
            .collect();

        let mut cli = self.cli.clone();

        // ✅ 使用你贴出的完整签名
        let candidates = complete(
            &mut cli,
            args_os,
            arg_index,
            Some(Path::new(".")), // current_dir
        )
        .unwrap_or_default();

        // ✅ CompletionCandidate 没有 display，只有 value (OsString)
        let pairs = candidates
            .into_iter()
            .map(|c| {
                let value = c.get_value().to_string_lossy().to_string();
                Pair {
                    display: value.clone(),
                    replacement: value,
                }
            })
            .collect();

        let start = line[..pos]
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);

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
    ref_name
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("no branch")
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
r#"=== built::info ===
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
"#,
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

fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            return cmd.run(sub_matches);
        }
    }
    Ok(())
}
fn print_copyright() {
    println!("Copyright (c) 2026 HZFY. All Rights Reserved.");
}
fn print_license() {
    let _ = io::stdout().write_all(include_str!("../../../LICENSE").as_bytes());
    println!();
    let _ = io::stdout().flush();
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    welcome(true);
    let cli = command::add_commands(
        Command::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .about(env!("CARGO_PKG_DESCRIPTION")),
    );

    let helper = ClapHelper { cli: cli.clone() };
    let mut rl = Editor::<ClapHelper, _>::new()?;
    rl.set_helper(Some(helper));
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
                eprintln!("Readline error: {}", err);
                break;
            }
        };

        let input = input.trim();
        if input.is_empty() {
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
            _ => {}
        }

        let args = match shell_words::split(input) {
            Ok(args) => args,
            Err(e) => {
                eprintln!("Parse error: {}", e);
                continue;
            }
        };

        let full_args: Vec<&str> = std::iter::once("tc-cli")
            .chain(args.iter().map(String::as_str))
            .collect();

        match cli.clone().try_get_matches_from(&full_args) {
            Ok(matches) => {
                if let Err(e) = dispatch(&matches) {
                    eprintln!("Error: {}", e);
                }
            }
            Err(err) => {
                eprintln!("{}", err);
            }
        }
    }

    Ok(())
}
