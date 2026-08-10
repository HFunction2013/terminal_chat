use clap::builder::ValueRange;
use clap::{Arg, ArgAction, Command};

#[derive(serde::Deserialize, Debug)]
struct Config {
    commands: Vec<CommandDef>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct CommandDef {
    name: String,
    about: String,
    args: Option<Vec<ArgDef>>,
    hidden: Option<bool>,
    subcommands: Option<Vec<Self>>,
    // multiple_values: Option<bool>,
    #[cfg(not(debug_assertions))]
    #[serde(default)]
    debug_only: bool,

    aliases: Option<Vec<String>>,
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
    // value_parser: Option<String>,
}

fn map_action(s: &str) -> ArgAction {
    match s {
        "set_true" => ArgAction::SetTrue,
        "append" => ArgAction::Append,
        "set_false" => ArgAction::SetFalse,
        "count" => ArgAction::Count,
        _ => ArgAction::Set,
    }
}

fn parse_num_args(s: &str) -> ValueRange {
    match s {
        "0.." => (0..).into(),
        "1.." => (1..).into(),
        "0..1" => (0..1).into(),
        "0..=" => (0..=usize::MAX).into(),
        val if let Ok(n) = val.parse::<usize>() => n.into(),
        _ => panic!("invalid num_args range: {s}"),
    }
}

fn build_command(c: CommandDef) -> Command {
    // 泄漏所有需要的字符串为 &'static str
    let name: &'static str = Box::leak(c.name.into_boxed_str());
    let about: &'static str = Box::leak(c.about.into_boxed_str());
    let mut cmd = Command::new(name).about(about);

    if let Some(aliases) = c.aliases {
        for alias in aliases {
            let alias: &'static str = Box::leak(alias.into_boxed_str());
            cmd = cmd.alias(alias);
        }
    }

    if c.hidden == Some(true) {
        cmd = cmd.hide(true);
    }

    if let Some(args) = c.args {
        for a in args {
            let arg_name: &'static str = Box::leak(a.name.into_boxed_str());
            let mut arg = Arg::new(arg_name);

            if let Some(c) = a.short {
                arg = arg.short(c);
            }
            if let Some(l) = a.long {
                let l: &'static str = Box::leak(l.into_boxed_str());
                arg = arg.long(l);
            }
            if let Some(h) = a.help {
                let h: &'static str = Box::leak(h.into_boxed_str());
                arg = arg.help(h);
            }
            if let Some(vn) = a.value_name {
                let vn: &'static str = Box::leak(vn.into_boxed_str());
                arg = arg.value_name(vn);
            }
            if a.required == Some(true) {
                arg = arg.required(true);
            }
            if let Some(n) = a.num_args {
                arg = arg.num_args(parse_num_args(&n));
            }
            if let Some(act) = a.action {
                arg = arg.action(map_action(&act));
            }
            if let Some(dv) = a.default_value {
                let dv: &'static str = Box::leak(dv.into_boxed_str());
                arg = arg.default_value(dv);
            }
            if let Some(cf) = a.conflicts_with {
                let cf: &'static str = Box::leak(cf.into_boxed_str());
                arg = arg.conflicts_with(cf);
            }

            cmd = cmd.arg(arg);
        }
    }

    if let Some(mut subcommands) = c.subcommands {
        subcommands.sort_by(|a, b| a.name.cmp(&b.name));
        for sub in subcommands {
            cmd = cmd.subcommand(build_command(sub));
        }
    }

    cmd
}
pub fn add_commands_from_yaml(yaml_content: &str) -> Command {
    let config: Config =
        serde_yaml::from_str(yaml_content).expect("YAML parse failed, check syntax");

    let mut commands = config.commands;

    #[cfg(not(debug_assertions))]
    commands.retain(|c| !c.debug_only);

    commands.sort_by(|a, b| a.name.cmp(&b.name));

    let mut cmd = Command::new("tc-cli");
    for c in commands {
        // 此处 c 已经是 owned
        cmd = cmd.subcommand(build_command(c));
    }
    cmd
}
