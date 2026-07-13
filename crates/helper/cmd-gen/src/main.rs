use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct Config {
    commands: Vec<CommandDef>,
}

#[derive(Debug, Deserialize)]
struct CommandDef {
    name: String,
    about: String,

    // ✅ 新增：默认 false，老 yaml 完全兼容
    #[serde(default)]
    debug_only: bool,
}

/// 把命令名转成 Rust 结构体名
/// setg → Setg
/// create_freq → CreateFreq
fn to_struct_name(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>()
        + "Command"
}

/// 生成单个命令模块
fn generate_module(name: &str, about: &str, debug_only: bool) -> String {
    let struct_name = to_struct_name(name);

    // ✅ 只在 debug_only 时插入，不影响原有结构
    let cfg_attr = if debug_only {
        "#[cfg(debug_assertions)]\n"
    } else {
        ""
    };

    format!(
        r#"{cfg_attr}// {name}.rs
// {about}
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;

pub struct {struct_name};

impl CommandExecutor for {struct_name} {{
    fn name(&self) -> &'static str {{
        "{name}"
    }}

    fn run(&self, _matches: &ArgMatches) -> Result<()> {{
        // TODO: {about}
        println!("Command `{name}` is not yet implemented.");
        Ok(())
    }}
}}
"#,
        cfg_attr = cfg_attr,
        name = name,
        struct_name = struct_name,
        about = about
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();

    let yaml_path = Path::new(&manifest)
        .parent() // crates/
        .unwrap()
        .parent() // workspace root
        .unwrap()
        .join("cli")
        .join("commands.yaml");
    println!("{}", yaml_path.display());

    if !yaml_path.exists() {
        eprintln!("❌ commands.yaml not found in current directory");
        std::process::exit(1);
    }

    let yaml = fs::read_to_string(&yaml_path)?;
    let config: Config = serde_yaml::from_str(&yaml)?;

    let out_dir = Path::new("./crates/cli/src/commands");
    if !out_dir.exists() {
        fs::create_dir_all(&out_dir)?;
    }

    let mut mod_entries = Vec::new();

    for cmd in &config.commands {
        let file_name = format!("{}.rs", cmd.name);
        let file_path = out_dir.join(&file_name);

        // ✅ 原有 skip 逻辑，一字不改
        if file_path.exists() {
            println!("[SKIP] {}", file_name);
        } else {
            let content = generate_module(&cmd.name, &cmd.about, cmd.debug_only);
            let mut f = File::create(&file_path)?;
            f.write_all(content.as_bytes())?;
            println!("[CREATE] {}", file_name);
        }

        mod_entries.push(cmd.name.clone());
    }

    // 生成 mod.rs
    let mut mod_rs = String::new();
    mod_rs.push_str(
r#"use clap::ArgMatches;
use anyhow::Result;

pub trait CommandExecutor {
    fn name(&self) -> &'static str;

    fn run(&self, matches: &ArgMatches) -> Result<()>;
}
"#);

    // ✅ 只在这里按 debug_only 决定是否加 cfg
    for cmd in &config.commands {
        if cmd.debug_only {
            mod_rs.push_str("#[cfg(debug_assertions)]\n");
        }
        mod_rs.push_str(&format!("pub mod {};\n", cmd.name));
    }

    mod_rs.push('\n');
    mod_rs.push_str("pub fn all_commands() -> Vec<Box<dyn CommandExecutor>> {\n");
    mod_rs.push_str("    vec![\n");

    for cmd in &config.commands {
        if cmd.debug_only {
            mod_rs.push_str("        #[cfg(debug_assertions)]\n");
        }
        mod_rs.push_str(&format!(
            "        Box::new({}::{}),\n",
            cmd.name,
            to_struct_name(&cmd.name)
        ));
    }

    mod_rs.push_str("    ]\n");
    mod_rs.push_str("}\n");

    fs::write(out_dir.join("mod.rs"), mod_rs)?;
    println!("[CREATE] mod.rs");

    println!("\nDone! {} commands processed.", mod_entries.len());

    Ok(())
}