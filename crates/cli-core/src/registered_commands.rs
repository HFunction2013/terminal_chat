//! 统一命令表与参数规格注册表。
//!
//! 首词命令、子命令与参数补全类别统一由 core 维护：宿主内置命令在表初始化时登记，
//! 插件 load 时从其 `command_yaml` 解析顶层命令 / 子命令 / 参数类型写入。宿主 TUI 与
//! REPL 只从本表查询命令名、子命令与参数规格，不再在宿主侧硬编码命令清单。
//!
//! 参数类型分两类：
//! - 内置类型（成员 / 远端路径 / 本地路径 / 选项 key / 插件 / 命名空间等）：宿主按既有
//!   快照与注册表补全；
//! - 插件自定义概念（本表不认识的类型名）：参数规格记录自定义类型名，宿主回退到归属
//!   插件的可选 `tc_complete_arg` 回调取候选。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use cli_core_types::ui::ui_field_kind;
use hook_macro::register_hook;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 单个参数规格（内部模型）。
struct ArgSpec {
    /// 参数名（保留供后续补全占位 / 提示使用，当前补全只读取 kind/custom）。
    #[allow(dead_code)]
    name: String,
    /// 内置补全类别序号（[`ui_field_kind`]）。
    kind: i32,
    /// 插件自定义类型名；非空表示该类型 core 未内置，需走插件 `complete_arg` 回调。
    custom: String,
    /// 是否为选项（布尔 flag 或带值 option：带 `short`/`long` 或声明 `action`）。
    /// 选项不占用命令行位置参数序号；`arg_spec` 只在位置参数中按序号取规格。
    option: bool,
}

/// 一条命令 / 子命令条目（内部模型）。
struct CommandEntry {
    name: String,
    /// 父命令名；顶层命令为 `None`。
    parent: Option<String>,
    args: Vec<ArgSpec>,
    /// 归属插件名；内置命令为空串。
    owner: String,
}

/// 宿主内置命令：本地分发，不归属任何插件，卸载插件时保留。
const BUILTIN_COMMANDS: &[&str] = &[
    "exit",
    "quit",
    "q",
    "clear",
    "version",
    "build_info",
    "about",
    "copyright",
    "license",
    "coffee",
    "saying",
    "help",
    "banner",
    "train",
];

static TABLE: LazyLock<Mutex<Vec<CommandEntry>>> = LazyLock::new(|| {
    let mut entries: Vec<CommandEntry> = BUILTIN_COMMANDS
        .iter()
        .map(|name| CommandEntry {
            name: (*name).to_string(),
            parent: None,
            args: Vec::new(),
            owner: String::new(),
        })
        .collect();
    // 插件条目在 load 时追加。
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Mutex::new(entries)
});

/// 把 yaml 声明的类型名映射为 (内置类别序号, 自定义类型名)；未识别类型落入自定义。
fn map_type(raw: &str) -> (i32, String) {
    match raw.trim().to_uppercase().as_str() {
        "PLAIN" | "TEXT" | "STRING" => (ui_field_kind::PLAIN, String::new()),
        "MEMBER" => (ui_field_kind::MEMBER, String::new()),
        "REMOTE_PATH" | "REMOTE" => (ui_field_kind::REMOTE_PATH, String::new()),
        "LOCAL_PATH" | "LOCAL" => (ui_field_kind::LOCAL_PATH, String::new()),
        "OPTION_KEY" | "OPTION" => (ui_field_kind::OPTION_KEY, String::new()),
        "SUBCOMMAND" => (ui_field_kind::SUBCOMMAND, String::new()),
        "PLUGIN" => (ui_field_kind::PLUGIN, String::new()),
        "NAMESPACE" => (ui_field_kind::NAMESPACE, String::new()),
        "NO_COMPLETION" | "NONE" => (ui_field_kind::NO_COMPLETION, String::new()),
        _ => (ui_field_kind::PLAIN, raw.trim().to_string()),
    }
}

#[derive(serde::Deserialize)]
struct YamlDoc {
    commands: Vec<YamlCmd>,
}

#[derive(serde::Deserialize)]
struct YamlCmd {
    name: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    args: Vec<YamlArg>,
    #[serde(default)]
    subcommands: Vec<YamlCmd>,
}

#[derive(serde::Deserialize)]
struct YamlArg {
    name: String,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    short: Option<String>,
    #[serde(default)]
    long: Option<String>,
    #[serde(default)]
    action: Option<String>,
}

impl YamlCmd {
    /// 递归展开为命令条目，`parent` 为父命令名（顶层传 `None`）。
    fn into_entries(self, parent: Option<String>, owner: &str, out: &mut Vec<CommandEntry>) {
        // `hidden: true` 命令不进入补全候选表（仍可由插件正常执行）。
        if self.hidden {
            return;
        }
        let args = self
            .args
            .into_iter()
            .map(|a| {
                let (kind, custom) = a
                    .r#type
                    .as_deref()
                    .map(map_type)
                    .unwrap_or((ui_field_kind::PLAIN, String::new()));
                // 带短/长选项名或声明 action 的是选项（flag / 带值 option），不计入位置参数序号。
                let option = a.short.is_some() || a.long.is_some() || a.action.is_some();
                ArgSpec { name: a.name, kind, custom, option }
            })
            .collect();
        let name = self.name.clone();
        for sub in self.subcommands {
            sub.into_entries(Some(name.clone()), owner, out);
        }
        out.push(CommandEntry { name, parent, args, owner: owner.to_string() });
    }
}

/// 解析插件 `command_yaml` 并写入命令表（归属 `owner`）。解析失败容错为空，不阻断加载。
pub(crate) fn register_yaml(yaml: &str, owner: &str) {
    let doc: YamlDoc = match serde_yaml::from_str(yaml) {
        Ok(doc) => doc,
        Err(_) => return,
    };
    let mut entries: Vec<CommandEntry> = Vec::new();
    for cmd in doc.commands {
        cmd.into_entries(None, owner, &mut entries);
    }
    let mut table = TABLE.lock().unwrap();
    for entry in entries {
        let exists = table
            .iter()
            .any(|e| e.name == entry.name && e.parent == entry.parent && e.owner == entry.owner);
        if !exists {
            table.push(entry);
        }
    }
}

/// 移除归属为 `owner` 的全部命令条目（卸载插件时调用）；内置条目保留。
pub(crate) fn unregister_by_owner(owner: &str) {
    TABLE.lock().unwrap().retain(|e| e.owner != owner);
}

/// 清空插件命令条目（内置命令保留）。
pub(crate) fn clear_all() {
    TABLE.lock().unwrap().retain(|e| e.owner.is_empty());
}

/// 在表内按命令路径（空格分隔，如 `plugin unload`）定位条目。
///
/// 宿主传入的 committed 路径可能已包含填好的参数词；逐层匹配时，遇到找不到对应
/// 子命令的词即视为参数起点并停止。返回条目与其命令层级词数（顶层命令为 1）。
fn locate<'a>(table: &'a [CommandEntry], path: &str) -> Option<(&'a CommandEntry, usize)> {
    let parts: Vec<&str> = path.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let mut parent: Option<&str> = None;
    let mut current: Option<&CommandEntry> = None;
    let mut depth = 0usize;
    for &part in &parts {
        match table.iter().find(|e| e.name == part && e.parent.as_deref() == parent) {
            Some(entry) => {
                current = Some(entry);
                parent = Some(part);
                depth += 1;
            }
            None => break,
        }
    }
    current.map(|entry| (entry, depth))
}

/// 列出全部首词命令名（顶层命令，排序去重）。
#[register_hook]
fn list_commands(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let table = TABLE.lock().unwrap();
    let mut names: Vec<String> =
        table.iter().filter(|e| e.parent.is_none()).map(|e| e.name.clone()).collect();
    // 已加载插件的命令命名空间（command_name）同样可作为首词路由执行，并入顶层候选。
    names.extend(crate::plugins::all_command_namespaces());
    names.sort();
    names.dedup();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        names.into_iter().map(|n| n.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出指定父命令的直接子命令名（排序去重）。
#[register_hook]
fn list_subcommands(
    parent: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result {
    let table = TABLE.lock().unwrap();
    let mut names: Vec<String> = table
        .iter()
        .filter(|e| e.parent == Some(parent.to_string()))
        .map(|e| e.name.clone())
        .collect();
    names.sort();
    names.dedup();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        names.into_iter().map(|n| n.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 查询命令路径在某参数位置的规格：出参一行 `kind|custom`；未命中返回空缓冲。
/// `arg_index` 为命令后第几个参数（1 起）。
#[register_hook]
fn arg_spec(
    command_path: &safer_ffi::String,
    arg_index: &i32,
    out: Out<'_, safer_ffi::String>,
) -> Result {
    let table = TABLE.lock().unwrap();
    let row = match locate(&table, &command_path.to_string()) {
        Some((entry, depth)) => {
            // 传入序号对应整条 committed 路径；前 depth 个词是命令层级，剩余为已填参数。
            // 选项（flag / 带值 option）不占位置参数序号，只在位置参数中按序号取规格。
            let param_index = (*arg_index - depth as i32).max(0) as usize;
            entry
                .args
                .iter()
                .filter(|a| !a.option)
                .nth(param_index)
                .map(|a| format!("{}|{}", a.kind, a.custom))
                .unwrap_or_default()
        }
        None => String::new(),
    };
    out.write(row.into());
    Result::success()
}

/// 参数补全转发：按首词路由到归属插件，调用其可选 `tc_complete_arg` 回调。
/// 插件未导出符号或未命中归属时回写空候选，不阻断宿主内置类型补全。
#[register_hook]
fn complete_arg(
    argv: &safer_ffi::Vec<safer_ffi::String>,
    arg_index: &i32,
    prefix: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result {
    let first = argv.first().map(|s| s.to_string()).unwrap_or_default();
    let meta = crate::plugins::command_name_registered(&first)
        .then(|| crate::plugins::plugin_by_command_name(&first))
        .flatten()
        .or_else(|| crate::plugins::global_plugin_by_command(&first));

    let Some(meta) = meta else {
        out.write(Vec::new().into());
        return Result::success();
    };

    let Some(lib) = crate::plugins::get_plugin_library(&meta.name.to_string()) else {
        out.write(Vec::new().into());
        return Result::success();
    };

    let complete: cli_core_types::plugin_fn_types::CompleteArgFn = match unsafe {
        lib.get::<cli_core_types::plugin_fn_types::CompleteArgFn>(b"tc_complete_arg")
    } {
        Ok(f) => *f,
        Err(_) => {
            out.write(Vec::new().into());
            return Result::success();
        }
    };

    complete(argv, *arg_index, prefix, out);
    Result::success()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn list_rows<F>(fill: F) -> Vec<String>
    where
        F: FnOnce(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>),
    {
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        fill(Out::from(&mut buf));
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builtin_commands_listed_without_owner() {
        let rows = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        assert!(rows.iter().any(|c| c == "exit"));
        assert!(rows.iter().any(|c| c == "help"));
    }

    #[test]
    fn list_commands_include_plugin_namespace() {
        let mut meta = cli_core_types::PluginMetadata::default();
        meta.name = "ns-plug".into();
        meta.command_name = "nst".into();
        meta.command_yaml = "commands:\n  - name: hello\n".into();
        unsafe {
            let _ = crate::plugins::register_plugin(&meta);
        }
        let rows = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        // 顶层命令名与可作为首词路由的命名空间都应进入候选。
        assert!(rows.iter().any(|c| c == "hello"));
        assert!(rows.iter().any(|c| c == "nst"));
        unsafe {
            let _ = crate::plugins::unregister_plugin(&"ns-plug".into());
        }
        let after = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        assert!(!after.iter().any(|c| c == "nst"));
        assert!(!after.iter().any(|c| c == "hello"));
    }

    #[test]
    fn yaml_registers_commands_subcommands_and_types() {
        let yaml = r#"
commands:
  - name: set
    args:
      - name: key
        type: OPTION_KEY
  - name: proxy
    subcommands:
      - name: on
      - name: off
  - name: join
    args:
      - name: user
        type: USER
"#;
        register_yaml(yaml, "test-plug");

        let cmds = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        assert!(cmds.iter().any(|c| c == "set"));
        assert!(cmds.iter().any(|c| c == "proxy"));

        let subs = list_rows(|out| unsafe {
            let _ = list_subcommands(&"proxy".into(), out);
        });
        assert_eq!(subs, vec!["off".to_string(), "on".to_string()]);

        // 内置类型：custom 为空。
        let mut spec: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"set".into(), &1, Out::from(&mut spec));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec), "4|");

        // 自定义类型 USER：kind 落 PLAIN，custom 保留类型名。
        let mut spec2: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"join".into(), &1, Out::from(&mut spec2));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec2), "0|USER");

        unregister_by_owner("test-plug");
        let after = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        assert!(!after.iter().any(|c| c == "set"));
        assert!(!after.iter().any(|c| c == "proxy"));
    }

    #[test]
    fn arg_spec_tolerates_trailing_filled_args() {
        register_yaml(
            r#"
commands:
  - name: mv
    args:
      - name: src
        type: REMOTE_PATH
      - name: dst
        type: REMOTE_PATH
"#,
            "t",
        );
        // committed 为 `mv src`，当前在第二参位置；core 需截断到命令 `mv` 并取 dst 规格。
        let mut spec: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"mv src".into(), &2, Out::from(&mut spec));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec), "2|");
        unregister_by_owner("t");
    }

    #[test]
    fn flags_do_not_occupy_positional_arg_index() {
        register_yaml(
            r#"
commands:
  - name: load
    subcommands:
      - name: plugin
        args:
          - name: load_namespace
            long: load-namespace
            action: set_true
          - name: plugin
            type: PLUGIN
  - name: unsetg
    args:
      - name: key
        type: OPTION_KEY
      - name: all
        long: all
        action: set_true
"#,
            "flag-plug",
        );
        // `load plugin <plugin>`：flag 在前，位置参数 plugin 仍解析为 PLUGIN(6)。
        let mut spec: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"load plugin".into(), &2, Out::from(&mut spec));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec), "6|");

        // `unsetg <key>`：第一个位置参数为 OPTION_KEY(4)；`all` 是 flag，第二位置无规格。
        let mut spec2: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"unsetg".into(), &1, Out::from(&mut spec2));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec2), "4|");

        let mut spec3: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = arg_spec(&"unsetg".into(), &2, Out::from(&mut spec3));
        }
        assert_eq!(&*ManuallyDrop::into_inner(spec3), "");

        unregister_by_owner("flag-plug");
    }

    #[test]
    fn hidden_command_is_not_listed_as_completion() {
        register_yaml(
            r#"
commands:
  - name: visible
  - name: hidden-cmd
    hidden: true
"#,
            "hidden-plug",
        );
        let rows = list_rows(|out| unsafe {
            let _ = list_commands(out);
        });
        assert!(rows.iter().any(|c| c == "visible"));
        assert!(!rows.iter().any(|c| c == "hidden-cmd"));
        unregister_by_owner("hidden-plug");
    }
}
