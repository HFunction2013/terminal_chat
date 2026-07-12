use clap::Command;
use clap_complete::engine::complete;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::hint::Hinter;
use rustyline::highlight::Highlighter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use shell_words;
use std::ffi::OsString;
use std::path::Path;
use chrono::DateTime;
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
mod command {
    include!(concat!(env!("OUT_DIR"), "/command.rs"));
}
fn print_banner() {
    println!("=== built::info ===");

    // ----- Cargo / Package -----
    println!("\n# Package");
    println!("PKG_NAME: {}", built_info::PKG_NAME);
    println!("PKG_VERSION: {}", built_info::PKG_VERSION);
    println!("PKG_VERSION_MAJOR: {}", built_info::PKG_VERSION_MAJOR);
    println!("PKG_VERSION_MINOR: {}", built_info::PKG_VERSION_MINOR);
    println!("PKG_VERSION_PATCH: {}", built_info::PKG_VERSION_PATCH);
    println!("PKG_VERSION_PRE: {}", built_info::PKG_VERSION_PRE);
    println!("PKG_AUTHORS: {}", built_info::PKG_AUTHORS);
    println!("PKG_DESCRIPTION: {}", built_info::PKG_DESCRIPTION);
    println!("PKG_HOMEPAGE: {}", built_info::PKG_HOMEPAGE);
    println!("PKG_LICENSE: {}", built_info::PKG_LICENSE);
    println!("PKG_REPOSITORY: {}", built_info::PKG_REPOSITORY);

    // ----- Build target -----
    println!("\n# Target");
    println!("TARGET: {}", built_info::TARGET);
    println!("HOST: {}", built_info::HOST);
    println!("PROFILE: {}", built_info::PROFILE);

    // ----- Toolchain -----
    println!("\n# Toolchain");
    println!("RUSTC: {}", built_info::RUSTC);
    println!("RUSTDOC: {}", built_info::RUSTDOC);
    println!("RUSTC_VERSION: {}", built_info::RUSTC_VERSION);
    println!("RUSTDOC_VERSION: {}", built_info::RUSTDOC_VERSION);

    // ----- Compile options -----
    println!("\n# Compile options");
    println!("OPT_LEVEL: {}", built_info::OPT_LEVEL);
    println!("NUM_JOBS: {}", built_info::NUM_JOBS);
    println!("DEBUG: {}", built_info::DEBUG);

    // ----- Features -----
    println!("\n# Features");
    println!("FEATURES: {:?}", built_info::FEATURES);
    println!("FEATURES_STR: {}", built_info::FEATURES_STR);
    println!("FEATURES_LOWERCASE: {:?}", built_info::FEATURES_LOWERCASE);
    println!("FEATURES_LOWERCASE_STR: {}", built_info::FEATURES_LOWERCASE_STR);

    // ----- CARGO_CFG_* -----
    println!("\n# CARGO_CFG");
    println!("CFG_TARGET_ARCH: {}", built_info::CFG_TARGET_ARCH);
    println!("CFG_ENDIAN: {}", built_info::CFG_ENDIAN);
    println!("CFG_ENV: {}", built_info::CFG_ENV);
    println!("CFG_FAMILY: {}", built_info::CFG_FAMILY);
    println!("CFG_OS: {}", built_info::CFG_OS);
    println!("CFG_POINTER_WIDTH: {}", built_info::CFG_POINTER_WIDTH);

    // ----- CI -----
    {
        println!("\n# CI");
        println!("CI_PLATFORM: {:?}", built_info::CI_PLATFORM);
    }

    // ----- Build time -----
    {
        println!("\n# Build time");
        println!("BUILT_TIME_UTC: {}", built_info::BUILT_TIME_UTC);
    }

    // ----- Git -----
    {
        println!("\n# Git");
        println!("GIT_VERSION: {:?}", built_info::GIT_VERSION);
        println!("GIT_DIRTY: {:?}", built_info::GIT_DIRTY);
        println!("GIT_HEAD_REF: {:?}", built_info::GIT_HEAD_REF);
        println!("GIT_COMMIT_HASH: {:?}", built_info::GIT_COMMIT_HASH);
        println!("GIT_COMMIT_HASH_SHORT: {:?}", built_info::GIT_COMMIT_HASH_SHORT);
    }

    // ----- Dependencies -----
    {
        println!("\n# Dependencies");
        println!("DEPENDENCIES_STR: {}", built_info::DEPENDENCIES_STR);
    }

    {
        println!("\n# Dependency tree");
        println!("DIRECT_DEPENDENCIES_STR: {}", built_info::DIRECT_DEPENDENCIES_STR);
        println!("INDIRECT_DEPENDENCIES_STR: {}", built_info::INDIRECT_DEPENDENCIES_STR);
    }

    println!("\n=== end of built::info ===");
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

        let args = match shell_words::split(before_cursor) {
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
            .filter(|c| !c.is_hide_set())
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = command::add_commands(
        Command::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .about(env!("CARGO_PKG_DESCRIPTION")),
    );

    let helper = ClapHelper { cli: cli.clone() };
    let mut rl = Editor::<ClapHelper, _>::new()?;
    rl.set_helper(Some(helper));
    print_banner();
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

        if input == "exit" {
            println!("Bye");
            break;
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
                println!("✅ Parsed:");
                println!("{:#?}", matches);
            }
            Err(err) => {
                eprintln!("{}", err);
            }
        }
    }

    Ok(())
}