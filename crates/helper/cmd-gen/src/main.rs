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
fn generate_module(name: &str, about: &str) -> String {
    let struct_name = to_struct_name(name);

    format!(
        r#"// {name}.rs
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

    let yaml = fs::read_to_string(yaml_path)?;
    let config: Config = serde_yaml::from_str(&yaml)?;

    let out_dir = Path::new("./crates/cli/src/commands");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir)?;
    }

    let mut mod_entries = Vec::new();

    for cmd in config.commands {
        let file_name = format!("{}.rs", cmd.name);
        let file_path = out_dir.join(&file_name);

        // 已存在则跳过
        if file_path.exists() {
            println!("[SKIP] {}", file_name);
        } else {
            let content = generate_module(&cmd.name, &cmd.about);
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
    /// 对应 YAML 里的 command.name
    fn name(&self) -> &'static str;

    /// 执行命令
    fn run(&self, matches: &ArgMatches) -> Result<()>;
}
"#);
    for name in &mod_entries {
        mod_rs.push_str(&format!("pub mod {};\n", name));
    }

    mod_rs.push('\n');
    mod_rs.push_str("pub fn all_commands() -> Vec<Box<dyn CommandExecutor>> {\n");
    mod_rs.push_str("    vec![\n");

    for name in &mod_entries {
        mod_rs.push_str(&format!(
            "        Box::new({}::{}),\n",
            name,
            to_struct_name(&name)
        ));
    }

    mod_rs.push_str("    ]\n");
    mod_rs.push_str("}\n");

    fs::write(out_dir.join("mod.rs"), mod_rs)?;
    println!("[CREATE] mod.rs");

    println!("\nDone! {} commands processed.", mod_entries.len());

    Ok(())
}
