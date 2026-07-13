use built::write_built_file;
use std::fs;
use std::path::Path;

#[derive(serde::Deserialize, Debug)]
struct Config {
    commands: Vec<CommandDef>,
    aliases: Option<Vec<AliasDef>>,
}

#[derive(serde::Deserialize, Debug)]
struct AliasDef {
    name: String,
    alias: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct CommandDef {
    name: String,
    about: String,
    args: Option<Vec<ArgDef>>,
    hidden: Option<bool>,
    subcommands: Option<Vec<CommandDef>>,
    multiple_values: Option<bool>,

    // ✅ 仅新增：默认 false，老 yaml 完全兼容
    #[serde(default)]
    #[allow(dead_code)]
    debug_only: bool,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct ArgDef {
    name: String,
    short: Option<char>,
    long: Option<String>,
    help: Option<String>,
    value_name: Option<String>,
    required: Option<bool>,
    num_args: Option<String>,
    action: Option<String>,
    default_value: Option<String>,
    conflicts_with: Option<String>,
}

/// 映射action字符串到clap枚举
fn map_action(s: &str) -> &'static str {
    match s {
        "set_true" => "clap::ArgAction::SetTrue",
        "append" => "clap::ArgAction::Append",
        "set_false" => "clap::ArgAction::SetFalse",
        "count" => "clap::ArgAction::Count",
        _ => "clap::ArgAction::Set",
    }
}

/// 解析num_args范围字符串，转为rust合法代码
fn parse_num_args(s: &str) -> String {
    match s {
        "0.." => "0..".to_string(),
        "1.." => "1..".to_string(),
        "0..1" => "0..1".to_string(),
        "0..=" => "0..=".to_string(),
        val if val.parse::<usize>().is_ok() => val.to_string(),
        _ => panic!("invalid num_args range: {}", s),
    }
}

/// 转义字符串，避免生成代码双引号逃逸
fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 递归构建Command代码字符串
fn build_command(c: &CommandDef, global_aliases: &[(String, String)]) -> String {
    let mut code = String::new();
    let about = escape_str(&c.about);
    code.push_str(&format!(
        "Command::new(\"{}\").about(\"{}\")",
        escape_str(&c.name),
        about
    ));

    // alias
    for (cmd_name, alias) in global_aliases {
        if c.name == *cmd_name {
            code.push_str(&format!(".alias(\"{}\")", escape_str(alias)));
        }
    }

    if c.hidden == Some(true) {
        code.push_str(".hide(true)");
    }

    // exclusive group
    if c.multiple_values == Some(false)
        && let Some(args) = &c.args
    {
        let mut group = format!(
            "clap::ArgGroup::new(\"{}_exclusive\").multiple(false)",
            escape_str(&c.name)
        );
        for arg in args {
            group.push_str(&format!(".arg(\"{}\")", escape_str(&arg.name)));
        }
        code.push_str(&format!(".group({})", group));
    }

    // args
    if let Some(args) = &c.args {
        for a in args {
            let mut arg = format!("Arg::new(\"{}\")", escape_str(&a.name));

            if let Some(c) = a.short {
                arg.push_str(&format!(".short('{}')", c));
            }
            if let Some(l) = &a.long {
                arg.push_str(&format!(".long(\"{}\")", escape_str(l)));
            }
            if let Some(h) = &a.help {
                arg.push_str(&format!(".help(\"{}\")", escape_str(h)));
            }
            if let Some(vn) = &a.value_name {
                arg.push_str(&format!(".value_name(\"{}\")", escape_str(vn)));
            }
            if let Some(true) = a.required {
                arg.push_str(".required(true)");
            }
            if let Some(n) = &a.num_args {
                let range = parse_num_args(n);
                arg.push_str(&format!(".num_args({})", range));
            }
            if let Some(act) = &a.action {
                arg.push_str(&format!(".action({})", map_action(act)));
            }
            if let Some(dv) = &a.default_value {
                arg.push_str(&format!(".default_value(\"{}\")", escape_str(dv)));
            }
            if let Some(cf) = &a.conflicts_with {
                arg.push_str(&format!(".conflicts_with(\"{}\")", escape_str(cf)));
            }

            code.push_str(&format!(".arg({})", arg));
        }
    }

    // ✅ 子命令排序（关键）
    if let Some(mut subcommands) = c.subcommands.clone() {
        subcommands.sort_by(|a, b| a.name.cmp(&b.name));

        for sub in subcommands {
            let sub_code = build_command(&sub, global_aliases);
            code.push_str(&format!(".subcommand({})", sub_code));
        }
    }

    code
}

fn main() {
    write_built_file().unwrap();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("command.rs");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).expect("Failed create command dir");
    }

    let yaml_path = Path::new("./commands.yaml");
    let yaml = fs::read_to_string(yaml_path).expect("Cannot read ./commands.yaml, file missing");
    let config: Config =
        serde_yaml::from_str(&yaml).expect("commands.yaml yaml parse failed, check syntax");

    // 转换别名列表
    let alias_list: Vec<(String, String)> = config
        .aliases
        .as_ref()
        .map(|list| {
            list.iter()
                .map(|a| (a.name.clone(), a.alias.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut commands = config.commands;

    // ✅ 关键：release 下直接丢弃 debug_only 命令
    #[cfg(not(debug_assertions))]
    commands.retain(|c| !c.debug_only);

    commands.sort_by(|a, b| a.name.cmp(&b.name));

    let mut code = String::new();
    code.push_str("use clap::{Arg, Command};\n");
    code.push_str("pub fn add_commands(mut cmd: Command) -> Command {\n");

    for c in commands {
        let cmd_code = build_command(&c, &alias_list);
        code.push_str(&format!("    cmd = cmd.subcommand({});\n", cmd_code));
    }

    code.push_str("    cmd\n");
    code.push_str("}\n");

    // 仅变更时写入
    let need_write = if dest.exists() {
        match fs::read_to_string(&dest) {
            Ok(old) => old != code,
            Err(_) => true,
        }
    } else {
        true
    };

    if need_write {
        fs::write(&dest, code).expect("Failed write generated cli code");
    }

    println!("cargo:rerun-if-changed={}", yaml_path.display());
}