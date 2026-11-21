//! 宿主 UI 注册表读取与解析（cli 侧）。
//!
//! cli-core 注册表由插件在初始化阶段登记菜单 / 侧栏 / 底部条目；跨 FFI 查询经
//! `Out<Vec<String>>` 回传扁平记录行（字段以 `|` 分隔）。本模块把扁平行解析为
//! 宿主内部 owned UI 模型，供 TUI 渲染层消费。
//!
//! 过渡阶段：注册表为空时 TUI 仍回退现有内置硬编码菜单；本模块只负责读取与解析，
//! 不直接参与渲染。
// 渲染层接入前，下列模型 / FFI 别名暂未被生产代码引用；待菜单通路接线后移除。
#![allow(dead_code)]

use cli_core_types::Result;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

/// 列出已注册菜单项的 FFI 符号类型（cli-core `get_menu_items`）。
pub type GetMenuItemsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result;

/// 按命令名取表单规格的 FFI 符号类型（cli-core `get_form_spec`）。
pub type GetFormSpecFn = unsafe extern "C" fn(
    &safer_ffi::String,
    Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>,
) -> Result;

/// 列出已注册侧栏大项的 FFI 符号类型（cli-core `get_side_sections`）。
pub type GetSideSectionsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result;

/// 列出已注册侧栏小项的 FFI 符号类型（cli-core `get_side_items`）。
pub type GetSideItemsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result;

/// 列出已注册底部片段的 FFI 符号类型（cli-core `get_bottom_fragments`）。
pub type GetBottomFragmentsFn =
    unsafe extern "C" fn(Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result;

/// 表单输入字段（owned 模型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegFormField {
    pub label: String,
    pub required: bool,
    pub secret: bool,
    /// 补全类别序号（对齐 `cli_core_types::ui::ui_field_kind`）。
    pub kind: i32,
}

/// 待提交命令的表单规格（owned 模型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegFormSpec {
    pub title: String,
    pub cmd: String,
    pub prefill: String,
    pub fields: Vec<RegFormField>,
}

/// 菜单项动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegMenuAction {
    /// 直接执行命令行参数。
    Run(Vec<String>),
    /// 表单动作：仅记录命令名，待渲染前经 `GetFormSpecFn` 补全字段。
    FormCmd(String),
    /// 表单动作：字段已补全。
    Form(RegFormSpec),
}

/// 已注册菜单项（owned 模型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegMenuItem {
    pub label: String,
    pub group: String,
    pub accel: String,
    pub action: RegMenuAction,
}

/// 已注册菜单组（owned 模型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegMenuGroup {
    pub name: String,
    pub accel: String,
    pub items: Vec<RegMenuItem>,
}

/// 侧栏可折叠大项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegSideSection {
    pub id: String,
    pub title: String,
    pub default_open: bool,
    pub source: String,
}

/// 侧栏条目模板：归属栏目 + 前缀符号 + 内容占位 + 高亮标记 + 主/次行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegSideItem {
    pub section: String,
    pub prefix: String,
    pub label: String,
    pub highlight: bool,
    pub line: i32,
}

/// 底部状态栏追加片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegBottomFragment {
    pub label: String,
}

/// 按 `|` 拆一行；不足 `len` 段时补空串。
fn split_row(row: &str, len: usize) -> Vec<&str> {
    let parts: Vec<&str> = row.split('|').collect();
    (0..len).map(|i| parts.get(i).copied().unwrap_or("")).collect()
}

fn bool_from_i32(s: &str) -> bool {
    s.trim() != "0"
}

/// 解析一行菜单项：`label|group|accel|kind|argv|form_cmd`。
fn parse_menu_item(row: &str) -> Option<RegMenuItem> {
    let p = split_row(row, 6);
    let label = p[0].trim().to_string();
    if label.is_empty() {
        return None;
    }
    let group = p[1].trim().to_string();
    let accel = p[2].trim().to_string();
    let kind: i32 = p[3].trim().parse().unwrap_or(0);
    let argv: Vec<String> = p[4].split_whitespace().map(str::to_string).collect();
    let form_cmd = p[5].trim().to_string();
    let action = if kind == cli_core_types::ui::ui_action_kind::FORM {
        if form_cmd.is_empty() {
            return None;
        }
        RegMenuAction::FormCmd(form_cmd)
    } else {
        RegMenuAction::Run(argv)
    };
    Some(RegMenuItem { label, group, accel, action })
}

/// 解析菜单扁平行，按组名聚合为有序菜单组；组顺序按条目首次出现。
pub fn parse_menu_rows(rows: &[String]) -> Vec<RegMenuGroup> {
    let mut groups: Vec<RegMenuGroup> = Vec::new();
    for row in rows {
        let Some(item) = parse_menu_item(row) else { continue };
        match groups.iter_mut().find(|g| g.name == item.group) {
            Some(group) => group.items.push(item),
            None => groups.push(RegMenuGroup {
                name: item.group.clone(),
                accel: item.accel.clone(),
                items: vec![item],
            }),
        }
    }
    groups
}

/// 解析表单规格行：首行 `SPEC|title|cmd|prefill`，其后 `FIELD|label|required|secret|kind`。
pub fn parse_form_spec_rows(rows: &[String]) -> Option<RegFormSpec> {
    let mut spec: Option<RegFormSpec> = None;
    for row in rows {
        let parts: Vec<&str> = row.split('|').collect();
        match parts.first().copied().unwrap_or("") {
            "SPEC" => {
                let p = split_row(row, 4);
                spec = Some(RegFormSpec {
                    title: p[1].trim().to_string(),
                    cmd: p[2].trim().to_string(),
                    // 前缀尾空格承载"命令词后接参数"的语义，须原样保留，不 trim。
                    prefill: p[3].to_string(),
                    fields: Vec::new(),
                });
            }
            "FIELD" => {
                let p = split_row(row, 5);
                let field = RegFormField {
                    label: p[1].trim().to_string(),
                    required: bool_from_i32(p[2]),
                    secret: bool_from_i32(p[3]),
                    kind: p[4].trim().parse().unwrap_or(0),
                };
                if let Some(spec) = spec.as_mut() {
                    spec.fields.push(field);
                }
            }
            _ => {}
        }
    }
    spec
}

/// 解析侧栏大项行：`id|title|default_open|source`。
pub fn parse_side_section_rows(rows: &[String]) -> Vec<RegSideSection> {
    rows.iter()
        .filter_map(|row| {
            let p = split_row(row, 4);
            let id = p[0].trim().to_string();
            if id.is_empty() {
                return None;
            }
            Some(RegSideSection {
                id,
                title: p[1].trim().to_string(),
                default_open: bool_from_i32(p[2]),
                source: p[3].trim().to_string(),
            })
        })
        .collect()
}

/// 解析侧栏小项行：`section|prefix|label|highlight|line`。
/// 前缀符号的前后空格承载缩进语义，原样保留不 trim。
pub fn parse_side_item_rows(rows: &[String]) -> Vec<RegSideItem> {
    rows.iter()
        .filter_map(|row| {
            let p = split_row(row, 5);
            let section = p[0].trim().to_string();
            let prefix = p[1].to_string();
            let label = p[2].trim().to_string();
            if section.is_empty() || label.is_empty() {
                return None;
            }
            Some(RegSideItem {
                section,
                prefix,
                label,
                highlight: bool_from_i32(p[3]),
                line: p[4].trim().parse().unwrap_or(0),
            })
        })
        .collect()
}

/// 解析底部片段行：单行 `label`。
pub fn parse_bottom_rows(rows: &[String]) -> Vec<RegBottomFragment> {
    rows.iter()
        .map(|row| RegBottomFragment { label: row.trim().to_string() })
        .filter(|f| !f.label.is_empty())
        .collect()
}

/// 从出参缓冲取回字符串列表；符号缺失或调用失败时返回空列表。
fn fetch_rows<F>(
    f: Option<&F>,
    call: impl FnOnce(&F, Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result,
) -> Vec<String> {
    let Some(f) = f else { return Vec::new() };
    let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
        ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
    let _ = call(f, Out::from(&mut buf));
    ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
}

/// 拉取已注册菜单条目（解析后）。
pub fn fetch_menu_items(f: Option<&GetMenuItemsFn>) -> Vec<RegMenuGroup> {
    let rows = fetch_rows(f, |f, out| unsafe { f(out) });
    parse_menu_rows(&rows)
}

/// 拉取指定命令名的表单规格；未注册返回 `None`。
pub fn fetch_form_spec(cmd: &str, f: Option<&GetFormSpecFn>) -> Option<RegFormSpec> {
    let rows = fetch_rows(f, |f, out| unsafe { f(&cmd.into(), out) });
    parse_form_spec_rows(&rows)
}

/// 拉取已注册侧栏大项。
pub fn fetch_side_sections(f: Option<&GetSideSectionsFn>) -> Vec<RegSideSection> {
    let rows = fetch_rows(f, |f, out| unsafe { f(out) });
    parse_side_section_rows(&rows)
}

/// 拉取已注册侧栏小项。
pub fn fetch_side_items(f: Option<&GetSideItemsFn>) -> Vec<RegSideItem> {
    let rows = fetch_rows(f, |f, out| unsafe { f(out) });
    parse_side_item_rows(&rows)
}

/// 拉取已注册底部片段。
pub fn fetch_bottom_fragments(f: Option<&GetBottomFragmentsFn>) -> Vec<RegBottomFragment> {
    let rows = fetch_rows(f, |f, out| unsafe { f(out) });
    parse_bottom_rows(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_rows_aggregate_by_group() {
        let rows = vec![
            "Exit|File|E|0|exit|".to_string(),
            "Connect|Connection|C|0|connect|".to_string(),
            "Send...|Message|M|1||msg".to_string(),
        ];
        let groups = parse_menu_rows(&rows);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].name, "File");
        assert_eq!(groups[0].items[0].action, RegMenuAction::Run(vec!["exit".to_string()]));
        assert_eq!(groups[1].name, "Connection");
        assert_eq!(groups[2].items[0].action, RegMenuAction::FormCmd("msg".to_string()));
    }

    #[test]
    fn menu_row_rejects_empty_label_and_form_without_cmd() {
        let rows = vec!["|File|E|0|exit|".to_string(), "Broken|Message|M|1||".to_string()];
        assert!(parse_menu_rows(&rows).is_empty());
    }

    #[test]
    fn form_spec_rows_parse_header_and_fields() {
        let rows = vec![
            "SPEC|Join Frequency|join_freq|join_freq ".to_string(),
            "FIELD|Frequency ID:|1|0|1".to_string(),
            "FIELD|Password:|0|1|0".to_string(),
        ];
        let spec = parse_form_spec_rows(&rows).expect("spec parsed");
        assert_eq!(spec.title, "Join Frequency");
        assert_eq!(spec.cmd, "join_freq");
        assert_eq!(spec.prefill, "join_freq ");
        assert_eq!(spec.fields.len(), 2);
        assert!(spec.fields[0].required);
        assert!(!spec.fields[0].secret);
        assert_eq!(spec.fields[0].kind, 1);
        assert!(!spec.fields[1].required);
        assert!(spec.fields[1].secret);
    }

    #[test]
    fn side_and_bottom_rows_parse() {
        let sections = parse_side_section_rows(&[
            "sessions|Sessions|1|sessions".to_string(),
            "members|Members|0|members".to_string(),
        ]);
        assert_eq!(sections.len(), 2);
        assert!(sections[0].default_open);
        assert!(!sections[1].default_open);

        let items = parse_side_item_rows(&[
            "sessions|* |default|1|0".to_string(),
            "|empty|0|0".to_string(),
        ]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].section, "sessions");
        assert_eq!(items[0].prefix, "* ");
        assert_eq!(items[0].label, "default");
        assert!(items[0].highlight);
        assert_eq!(items[0].line, 0);

        let bottom = parse_bottom_rows(&["user@host:port".to_string(), "".to_string()]);
        assert_eq!(bottom.len(), 1);
    }

    #[test]
    fn fetch_without_symbol_returns_empty() {
        assert!(fetch_menu_items(None).is_empty());
        assert!(fetch_form_spec("msg", None).is_none());
        assert!(fetch_side_sections(None).is_empty());
        assert!(fetch_side_items(None).is_empty());
        assert!(fetch_bottom_fragments(None).is_empty());
    }
}
