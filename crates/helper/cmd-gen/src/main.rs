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

    #[serde(default)]
    debug_only: bool,

    #[serde(default)]
    subcommands: Vec<CommandDef>,
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

    let cfg_attr = if debug_only {
        "#[cfg(debug_assertions)]\n"
    } else {
        ""
    };

    // 把 about 转成 Rust 注释，每行加 //
    let comment_lines: Vec<&str> = about.lines().collect();
    let comment = comment_lines
        .iter()
        .map(|line| format!("// {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let todo = comment_lines
        .iter()
        .map(|line| format!("// TODO: {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"{cfg_attr}// {name}.rs
{comment}
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;

pub struct {struct_name};

impl CommandExecutor for {struct_name} {{
    fn name(&self) -> &'static str {{
        "{name}"
    }}

    fn run(&self, _matches: &ArgMatches) -> Result<()> {{
        {todo}
        println!("Command `{name}` is not yet implemented.");
        Ok(())
    }}
}}
"#,
        cfg_attr = cfg_attr,
        name = name,
        comment = comment,
        struct_name = struct_name,
        todo = todo
    )
}

/// 递归处理命令及其 subcommands
fn process_commands(
    cmds: &[CommandDef],
    parent_dir: &Path,
    mod_entries: &mut Vec<(String, bool)>,
    prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for cmd in cmds {
        let current_prefix = if prefix.is_empty() {
            cmd.name.clone()
        } else {
            format!("{}::{}", prefix, cmd.name)
        };

        let file_name = format!("{}.rs", cmd.name);
        let file_path = parent_dir.join(&file_name);

        if !cmd.subcommands.is_empty() {
            let sub_dir = parent_dir.join(&cmd.name);
            if !sub_dir.exists() {
                fs::create_dir_all(&sub_dir)?;
                println!("[MKDIR] {}", sub_dir.display());
            }

            mod_entries.push((current_prefix.clone(), true));

            process_commands(&cmd.subcommands, &sub_dir, mod_entries, &current_prefix)?;
        } else {
            if file_path.exists() {
                println!("[SKIP] {}", file_name);
            } else {
                let content = generate_module(&cmd.name, &cmd.about, cmd.debug_only);
                let mut f = File::create(&file_path)?;
                f.write_all(content.as_bytes())?;
                println!("[CREATE] {}", file_name);
            }

            mod_entries.push((current_prefix, false));
        }
    }
    Ok(())
}

/// 生成子命令列表的 all_commands 部分
fn generate_sub_all_commands(cmd: &CommandDef, indent: usize) -> String {
    let mut result = String::new();
    let indent_str = " ".repeat(indent);
    
    for sub in &cmd.subcommands {
        if sub.debug_only {
            result.push_str(&format!("{}#[cfg(debug_assertions)]\n", indent_str));
        }
        
        if sub.subcommands.is_empty() {
            let struct_name = to_struct_name(&sub.name);
            result.push_str(&format!(
                "{}Arc::new({}::{}),\n",
                indent_str, sub.name, struct_name
            ));
        } else {
            // 有子命令的，通过其 mod.rs 中的结构体访问
            let struct_name = to_struct_name(&sub.name);
            result.push_str(&format!(
                "{}Arc::new({}::{}),\n",
                indent_str, sub.name, struct_name
            ));
        }
    }
    
    result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();

    let yaml_path = Path::new(&manifest)
        .parent()
        .unwrap()
        .parent()
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

    process_commands(&config.commands, out_dir, &mut mod_entries, "")?;

    // 生成 mod.rs - 主入口
    let mut mod_rs = String::new();
    mod_rs.push_str(
        r#"use clap::ArgMatches;
use std::sync::Arc;
use anyhow::Result;
use std::sync::atomic::Ordering;
use crate::INTERRUPTED;

pub trait CommandExecutor {
    fn name(&self) -> &'static str;

    fn run(&self, matches: &ArgMatches) -> Result<()>;
}
"#,
    );

    // 生成顶级模块声明
    for (entry, _) in &mod_entries {
        let parts: Vec<&str> = entry.split("::").collect();

        if parts.len() == 1 {
            let cmd_name = &parts[0];

            fn find_cmd<'a>(cmds: &'a [CommandDef], name: &str) -> Option<&'a CommandDef> {
                cmds.iter().find(|c| c.name == name)
            }

            let cmd = find_cmd(&config.commands, cmd_name);

            if let Some(cmd) = cmd {
                if cmd.debug_only {
                    mod_rs.push_str("#[cfg(debug_assertions)]\n");
                }
            }

            mod_rs.push_str(&format!("pub mod {};\n", cmd_name));
        }
    }

    mod_rs.push('\n');
    mod_rs.push_str("pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {\n");
    mod_rs.push_str("    vec![\n");

    for (entry, _has_sub) in &mod_entries {
        // 只跳过嵌套的子命令（如 "parent::child"），保留所有顶级命令
        if entry.contains("::") {
            continue;
        }

        let parts: Vec<&str> = entry.split("::").collect();
        let cmd_name = parts[parts.len() - 1];

        fn find_cmd<'a>(cmds: &'a [CommandDef], name: &str) -> Option<&'a CommandDef> {
            cmds.iter().find(|c| c.name == name)
        }

        let cmd = find_cmd(&config.commands, cmd_name);

        if let Some(cmd) = cmd {
            if cmd.debug_only {
                mod_rs.push_str("        #[cfg(debug_assertions)]\n");
            }
        }

        let struct_name = to_struct_name(cmd_name);
        mod_rs.push_str(&format!(
            "        Arc::new({}::{}),\n",
            cmd_name, struct_name
        ));
    }

    mod_rs.push_str("    ]\n");
    mod_rs.push_str("}\n");

    mod_rs.push_str(
        r#"
pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            // 每次执行前清零
            INTERRUPTED.store(false, Ordering::SeqCst);
            return cmd.run(sub_matches);
        }
    }
    eprintln!("No matching command found. Use --help for usage information.");
    Ok(())
}
"#,
    );

    fs::write(out_dir.join("mod.rs"), mod_rs)?;
    println!("[CREATE] mod.rs (top-level)");

    // 为有子命令的文件夹生成内部的 mod.rs
    for (entry, has_sub) in &mod_entries {
        if *has_sub {
            let parts: Vec<&str> = entry.split("::").collect();
            let dir_name = parts.last().unwrap();

            let mut dir_path = out_dir.to_path_buf();
            for part in &parts {
                dir_path = dir_path.join(part);
            }

            let mut sub_mod_rs = String::new();

            fn find_cmd_by_path<'a>(
                cmds: &'a [CommandDef],
                parts: &[&str],
            ) -> Option<&'a CommandDef> {
                if parts.is_empty() {
                    return None;
                }
                let cmd = cmds.iter().find(|c| c.name == parts[0])?;
                if parts.len() == 1 {
                    Some(cmd)
                } else {
                    find_cmd_by_path(&cmd.subcommands, &parts[1..])
                }
            }

            let cmd = find_cmd_by_path(&config.commands, &parts);

            if let Some(cmd) = cmd {
                sub_mod_rs.push_str("use clap::ArgMatches;\n");
                sub_mod_rs.push_str("use std::sync::Arc;\n");
                sub_mod_rs.push_str("use anyhow::Result;\n");
                sub_mod_rs.push_str("use crate::commands::CommandExecutor;\n");
                sub_mod_rs.push_str("use crate::INTERRUPTED;\n");
                sub_mod_rs.push_str("use std::sync::atomic::Ordering;\n\n");

                // ==== 新增：导出父命令自身的 Executor ====
                let parent_struct_name = to_struct_name(dir_name);
                sub_mod_rs.push_str(&format!(
                    r#"pub struct {parent_struct_name};

impl CommandExecutor for {parent_struct_name} {{
    fn name(&self) -> &'static str {{
        "{dir_name}"
    }}

    fn run(&self, matches: &ArgMatches) -> Result<()> {{
        dispatch(matches)
    }}
}}

"#,
                    parent_struct_name = parent_struct_name,
                    dir_name = dir_name,
                ));
                // ========================================

                for sub in &cmd.subcommands {
                    if sub.debug_only {
                        sub_mod_rs.push_str("#[cfg(debug_assertions)]\n");
                    }

                    sub_mod_rs.push_str(&format!("pub mod {};\n", sub.name));
                }

                // ==== 新增：子目录的 all_commands ====
                sub_mod_rs.push_str(&format!(
                    r#"
pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {{
    vec![
{}
    ]
}}

"#,
                    generate_sub_all_commands(cmd, 8)
                ));
                // ========================================

                // ==== 修改：dispatch 使用 all_commands 方式 ====
                sub_mod_rs.push_str("pub fn dispatch(matches: &ArgMatches) -> Result<()> {\n");
                sub_mod_rs.push_str("    for cmd in all_commands() {\n");
                sub_mod_rs.push_str("        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {\n");
                sub_mod_rs.push_str("            INTERRUPTED.store(false, Ordering::SeqCst);\n");
                sub_mod_rs.push_str("            return cmd.run(sub_matches);\n");
                sub_mod_rs.push_str("        }\n");
                sub_mod_rs.push_str("    }\n");
                sub_mod_rs.push_str("    eprintln!(\"No matching command found. Use --help for usage information.\");\n");
                
                sub_mod_rs.push_str("    Ok(())\n");
                sub_mod_rs.push_str("}\n");
            }

            fs::write(dir_path.join("mod.rs"), sub_mod_rs)?;
            println!("[CREATE] {}/mod.rs", dir_name);
        }
    }

    println!("\nDone! {} commands processed.", mod_entries.len());

    Ok(())
}