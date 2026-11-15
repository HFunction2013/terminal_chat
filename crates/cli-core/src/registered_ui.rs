//! 宿主 UI 注册表（菜单 / 侧栏 / 底部片段）。
//!
//! 插件在初始化阶段通过注册原语登记菜单项、侧栏可折叠大项 / 小项与底部栏片段；
//! 宿主 cli 只从注册表渲染，不内嵌具体插件的菜单 / 侧栏知识。
//!
//! 受当前 safer_ffi 版本限制，ReprC 结构体内不能内嵌 `Vec`（`String` 为 Opaque），
//! 故条目模型保留在本模块内部；跨 FFI 注册原语只收标量 / 字符串，查询经
//! `Out<Vec<String>>` 回传扁平记录行（字段以 `|` 分隔）。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 表单输入字段（内部模型）。
#[derive(Clone, Debug, PartialEq)]
struct FormField {
    label: String,
    required: bool,
    secret: bool,
    kind: i32,
}

/// 待提交命令的表单规格（内部模型）。
#[derive(Clone, Debug, PartialEq, Default)]
struct FormSpec {
    title: String,
    cmd: String,
    prefill: String,
    fields: Vec<FormField>,
}

/// 菜单项动作。
#[derive(Clone, Debug, PartialEq)]
enum MenuAction {
    /// 直接执行命令行参数。
    Run(Vec<String>),
    /// 弹出参数表单；按 `cmd` 关联已注册字段。
    Form(FormSpec),
}

/// 已注册菜单项（内部模型）。
#[derive(Clone, Debug, PartialEq)]
struct MenuItem {
    label: String,
    group: String,
    accel: String,
    action: MenuAction,
}

/// 侧栏可折叠大项（栏目壳）。
#[derive(Clone, Debug, PartialEq)]
struct SideSection {
    id: String,
    title: String,
    default_open: bool,
    source: String,
}

/// 侧栏小项。
#[derive(Clone, Debug, PartialEq)]
struct SideItem {
    section: String,
    label: String,
}

static MENU_ITEMS: LazyLock<Mutex<Vec<MenuItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static FORM_SPECS: LazyLock<Mutex<HashMap<String, FormSpec>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SIDE_SECTIONS: LazyLock<Mutex<Vec<SideSection>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static SIDE_ITEMS: LazyLock<Mutex<Vec<SideItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static BOTTOM_FRAGMENTS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 拆分命令行字符串为参数（空白分隔）。
fn split_argv(argv: &str) -> Vec<String> {
    argv.split_whitespace().map(|s| s.to_string()).collect()
}

/// 登记一个菜单项（幂等：同标签 / 组 / 动作已存在则不重复插入）。
///
/// `action_kind` 见 [`cli_core_types::ui::ui_action_kind`]；`argv` 为空格分隔命令行；
/// `form_cmd` 非空时按表单动作关联已注册字段。
#[register_hook]
fn register_menu_item(
    label: &safer_ffi::String,
    group: &safer_ffi::String,
    accel: &safer_ffi::String,
    action_kind: &i32,
    argv: &safer_ffi::String,
    form_cmd: &safer_ffi::String,
) -> Result {
    let action = if *action_kind == cli_core_types::ui::ui_action_kind::FORM {
        let spec =
            FORM_SPECS.lock().unwrap().get(&form_cmd.to_string()).cloned().unwrap_or_default();
        MenuAction::Form(spec)
    } else {
        MenuAction::Run(split_argv(&argv.to_string()))
    };
    let item = MenuItem {
        label: label.to_string(),
        group: group.to_string(),
        accel: accel.to_string(),
        action,
    };
    let mut items = MENU_ITEMS.lock().unwrap();
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
    Result::success()
}

/// 登记表单骨架（标题 / 命令 / 输入框前缀）；字段由 [`register_form_field`] 按
/// `form_cmd` 追加。已存在时只补全标题 / 前缀，不清空已登记字段。
#[register_hook]
fn register_form_spec(
    title: &safer_ffi::String,
    cmd: &safer_ffi::String,
    prefill: &safer_ffi::String,
) -> Result {
    let mut specs = FORM_SPECS.lock().unwrap();
    let spec = specs.entry(cmd.to_string()).or_default();
    spec.title = title.to_string();
    spec.cmd = cmd.to_string();
    spec.prefill = prefill.to_string();
    Result::success()
}

/// 登记表单字段，按 `form_cmd` 关联到对应表单规格。
#[register_hook]
fn register_form_field(
    form_cmd: &safer_ffi::String,
    label: &safer_ffi::String,
    required: &i32,
    secret: &i32,
    kind: &i32,
) -> Result {
    let mut specs = FORM_SPECS.lock().unwrap();
    let spec = specs.entry(form_cmd.to_string()).or_default();
    spec.fields.push(FormField {
        label: label.to_string(),
        required: *required != 0,
        secret: *secret != 0,
        kind: *kind,
    });
    Result::success()
}

/// 登记一个侧栏可折叠大项（栏目壳）。
#[register_hook]
fn register_side_section(
    id: &safer_ffi::String,
    title: &safer_ffi::String,
    default_open: &i32,
    source: &safer_ffi::String,
) -> Result {
    let section = SideSection {
        id: id.to_string(),
        title: title.to_string(),
        default_open: *default_open != 0,
        source: source.to_string(),
    };
    let mut sections = SIDE_SECTIONS.lock().unwrap();
    if !sections.iter().any(|s| s.id == section.id) {
        sections.push(section);
    }
    Result::success()
}

/// 往指定大项下登记一个侧栏小项。
#[register_hook]
fn register_side_item(section: &safer_ffi::String, label: &safer_ffi::String) -> Result {
    let item = SideItem { section: section.to_string(), label: label.to_string() };
    let mut items = SIDE_ITEMS.lock().unwrap();
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
    Result::success()
}

/// 登记一个底部状态栏追加片段。
#[register_hook]
fn register_bottom(label: &safer_ffi::String) -> Result {
    let mut fragments = BOTTOM_FRAGMENTS.lock().unwrap();
    if !fragments.iter().any(|f| f == &label.to_string()) {
        fragments.push(label.to_string());
    }
    Result::success()
}

/// 列出全部已注册菜单项；每行 `label|group|accel|kind|argv|form_cmd`。
#[register_hook]
fn get_menu_items(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let items = MENU_ITEMS.lock().unwrap();
    let rows: Vec<String> = items
        .iter()
        .map(|item| {
            let (kind, argv, form_cmd) = match &item.action {
                MenuAction::Run(argv) => {
                    (cli_core_types::ui::ui_action_kind::RUN, argv.join(" "), String::new())
                }
                MenuAction::Form(spec) => {
                    (cli_core_types::ui::ui_action_kind::FORM, String::new(), spec.cmd.clone())
                }
            };
            format!("{}|{}|{}|{}|{}|{}", item.label, item.group, item.accel, kind, argv, form_cmd)
        })
        .collect();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出全部已注册侧栏大项；每行 `id|title|default_open|source`。
#[register_hook]
fn get_side_sections(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let sections = SIDE_SECTIONS.lock().unwrap();
    let rows: Vec<String> = sections
        .iter()
        .map(|s| format!("{}|{}|{}|{}", s.id, s.title, i32::from(s.default_open), s.source))
        .collect();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出全部已注册侧栏小项；每行 `section|label`。
#[register_hook]
fn get_side_items(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let items = SIDE_ITEMS.lock().unwrap();
    let rows: Vec<String> = items.iter().map(|i| format!("{}|{}", i.section, i.label)).collect();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出全部已注册底部片段；每行 `label`。
#[register_hook]
fn get_bottom_fragments(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let fragments = BOTTOM_FRAGMENTS.lock().unwrap();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        fragments.iter().map(|f| f.as_str().into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 按 `form_cmd` 取回表单规格：首行 `SPEC|title|cmd|prefill`，其后每行
/// `FIELD|label|required|secret|kind`；未注册时返回空缓冲。
#[register_hook]
fn get_form_spec(
    form_cmd: &safer_ffi::String,
    out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result {
    let specs = FORM_SPECS.lock().unwrap();
    let mut rows: Vec<String> = Vec::new();
    if let Some(spec) = specs.get(&form_cmd.to_string()) {
        rows.push(format!("SPEC|{}|{}|{}", spec.title, spec.cmd, spec.prefill));
        for f in &spec.fields {
            rows.push(format!(
                "FIELD|{}|{}|{}|{}",
                f.label,
                i32::from(f.required),
                i32::from(f.secret),
                f.kind
            ));
        }
    }
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    #[test]
    fn register_menu_item_is_idempotent() {
        unsafe {
            let _ = register_menu_item(
                &"Connect".into(),
                &"Connection".into(),
                &"C".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"connect".into(),
                &"".into(),
            );
            let _ = register_menu_item(
                &"Connect".into(),
                &"Connection".into(),
                &"C".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"connect".into(),
                &"".into(),
            );
        }
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_menu_items(Out::from(&mut buf));
        }
        let got: Vec<String> =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn form_fields_aggregate_by_cmd() {
        unsafe {
            let _ = register_form_field(&"join_freq".into(), &"Frequency ID:".into(), &1, &0, &0);
            let _ = register_form_field(&"join_freq".into(), &"Password:".into(), &0, &1, &0);
        }
        let specs = FORM_SPECS.lock().unwrap();
        let spec = specs.get("join_freq").expect("form spec created");
        assert_eq!(spec.fields.len(), 2);
        assert!(spec.fields[0].required);
        assert!(spec.fields[1].secret);
    }

    #[test]
    fn form_spec_round_trips_via_out() {
        unsafe {
            let _ =
                register_form_spec(&"Join Frequency".into(), &"join_rt".into(), &"join_rt ".into());
            let _ = register_form_field(&"join_rt".into(), &"Frequency ID:".into(), &1, &0, &0);
        }
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_form_spec(&"join_rt".into(), Out::from(&mut buf));
        }
        let rows: Vec<String> =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], "SPEC|Join Frequency|join_rt|join_rt ");
        assert_eq!(rows[1], "FIELD|Frequency ID:|1|0|0");
    }
}
