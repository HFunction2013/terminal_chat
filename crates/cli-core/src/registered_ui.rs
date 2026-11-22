//! 宿主 UI 注册表（菜单 / 侧栏 / 底部片段）。
//!
//! 插件在初始化阶段通过注册原语登记菜单项、侧栏可折叠大项 / 小项与底部栏片段；
//! 宿主 cli 只从注册表渲染，不内嵌具体插件的菜单 / 侧栏知识。
//!
//! 受当前 safer_ffi 版本限制，ReprC 结构体内不能内嵌 `Vec`（`String` 为 Opaque），
//! 故条目模型保留在本模块内部；跨 FFI 注册原语只收标量 / 字符串，查询经
//! `Out<Vec<String>>` 回传扁平记录行（字段以 `|` 分隔）。
//!
//! 可译字段（菜单 label/group、表单 title/field label、侧栏 title/label、底部
//! 静态片段 label）存的是 i18n key；注册时不固化译文快照，查询时按当前语言
//! 经 [`crate::i18n::translate`] 现译。命令名、选项 key、前缀符号、命令行参数、
//! 标识符等非译字段原样存储与输出。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use cli_core_types::ui::BottomFragmentFn;
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
    owner: String,
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
    owner: String,
}

/// 侧栏可折叠大项（栏目壳）。
#[derive(Clone, Debug, PartialEq)]
struct SideSection {
    id: String,
    title: String,
    default_open: bool,
    source: String,
    owner: String,
}

/// 侧栏小项（条目模板：归属栏目 + 前缀符号 + 内容占位；动态条目内容由宿主快照回填）。
/// `highlight` 标记该模板用于当前高亮条目（如当前会话整行着色）；`line` 区分
/// 主行（0）与次行（1，如插件名下方的命名空间行）。
#[derive(Clone, Debug, PartialEq)]
struct SideItem {
    section: String,
    prefix: String,
    label: String,
    highlight: bool,
    line: i32,
    owner: String,
}

static MENU_ITEMS: LazyLock<Mutex<Vec<MenuItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static FORM_SPECS: LazyLock<Mutex<HashMap<String, FormSpec>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SIDE_SECTIONS: LazyLock<Mutex<Vec<SideSection>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static SIDE_ITEMS: LazyLock<Mutex<Vec<SideItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static BOTTOM_FRAGMENTS: LazyLock<Mutex<Vec<(String, String)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
/// 底部片段逐帧回调表：(回调, 归属插件名)；宿主查询时逐个调用取当前文本。
static BOTTOM_FRAGMENT_FNS: LazyLock<Mutex<Vec<(BottomFragmentFn, String)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

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
    let owner = crate::plugins::current_owner().unwrap_or_default();
    let item = MenuItem {
        label: label.to_string(),
        group: group.to_string(),
        accel: accel.to_string(),
        action,
        owner,
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
    spec.owner = crate::plugins::current_owner().unwrap_or_default();
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
    if spec.owner.is_empty() {
        spec.owner = crate::plugins::current_owner().unwrap_or_default();
    }
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
        owner: crate::plugins::current_owner().unwrap_or_default(),
    };
    let mut sections = SIDE_SECTIONS.lock().unwrap();
    if !sections.iter().any(|s| s.id == section.id) {
        sections.push(section);
    }
    Result::success()
}

/// 往指定大项下登记一个侧栏条目模板：归属栏目 + 前缀符号 + 内容占位；
/// `highlight` 非 0 表示该模板用于高亮当前条目；`line` 0 为主行、1 为次行。
#[register_hook]
fn register_side_item(
    section: &safer_ffi::String,
    prefix: &safer_ffi::String,
    label: &safer_ffi::String,
    highlight: &i32,
    line: &i32,
) -> Result {
    let item = SideItem {
        section: section.to_string(),
        prefix: prefix.to_string(),
        label: label.to_string(),
        highlight: *highlight != 0,
        line: *line,
        owner: crate::plugins::current_owner().unwrap_or_default(),
    };
    let mut items = SIDE_ITEMS.lock().unwrap();
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
    Result::success()
}

/// 登记一个底部状态栏追加片段。
#[register_hook]
fn register_bottom(label: &safer_ffi::String) -> Result {
    let owner = crate::plugins::current_owner().unwrap_or_default();
    let mut fragments = BOTTOM_FRAGMENTS.lock().unwrap();
    if !fragments.iter().any(|(f, _)| f == &label.to_string()) {
        fragments.push((label.to_string(), owner));
    }
    Result::success()
}

/// 登记一个底部片段逐帧回调；宿主查询时调用已注册回调取当前片段文本。
/// 未注册回调的插件不参与逐帧查询。
#[register_hook]
fn register_bottom_callback(cb: &BottomFragmentFn) -> Result {
    let owner = crate::plugins::current_owner().unwrap_or_default();
    let mut fns = BOTTOM_FRAGMENT_FNS.lock().unwrap();
    if !fns.iter().any(|(existing, _)| std::ptr::fn_addr_eq(*existing, *cb)) {
        fns.push((*cb, owner));
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
            format!(
                "{}|{}|{}|{}|{}|{}",
                crate::i18n::translate(&item.label),
                crate::i18n::translate(&item.group),
                item.accel,
                kind,
                argv,
                form_cmd
            )
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
        .map(|s| {
            format!(
                "{}|{}|{}|{}",
                s.id,
                crate::i18n::translate(&s.title),
                i32::from(s.default_open),
                s.source
            )
        })
        .collect();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出全部已注册侧栏条目模板；每行 `section|prefix|label|highlight|line`。
#[register_hook]
fn get_side_items(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let items = SIDE_ITEMS.lock().unwrap();
    let rows: Vec<String> = items
        .iter()
        .map(|i| {
            format!(
                "{}|{}|{}|{}|{}",
                i.section,
                i.prefix,
                crate::i18n::translate(&i.label),
                i32::from(i.highlight),
                i.line
            )
        })
        .collect();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

/// 列出全部已注册底部片段：静态登记文本 + 各逐帧回调当前返回的非空文本；每行 `label`。
#[register_hook]
fn get_bottom_fragments(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let mut rows: Vec<String> = BOTTOM_FRAGMENTS
        .lock()
        .unwrap()
        .iter()
        .map(|(label, _)| crate::i18n::translate(label))
        .collect();
    for (cb, _) in BOTTOM_FRAGMENT_FNS.lock().unwrap().iter() {
        let text = unsafe { cb() }.to_string();
        if !text.trim().is_empty() {
            rows.push(text);
        }
    }
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        rows.into_iter().map(|r| r.into()).collect::<Vec<_>>().into();
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
        rows.push(format!(
            "SPEC|{}|{}|{}",
            crate::i18n::translate(&spec.title),
            spec.cmd,
            spec.prefill
        ));
        for f in &spec.fields {
            rows.push(format!(
                "FIELD|{}|{}|{}|{}",
                crate::i18n::translate(&f.label),
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

/// 移除归属为 `owner` 的全部 UI 注册项（卸载插件时调用）。
pub(crate) fn unregister_by_owner(owner: &str) {
    MENU_ITEMS.lock().unwrap().retain(|i| i.owner != owner);
    FORM_SPECS.lock().unwrap().retain(|_, s| s.owner != owner);
    SIDE_SECTIONS.lock().unwrap().retain(|s| s.owner != owner);
    SIDE_ITEMS.lock().unwrap().retain(|i| i.owner != owner);
    BOTTOM_FRAGMENTS.lock().unwrap().retain(|(_, o)| o != owner);
    BOTTOM_FRAGMENT_FNS.lock().unwrap().retain(|(_, o)| o != owner);
}

/// 清空全部 UI 注册项（清空插件时调用）。
pub(crate) fn clear_all() {
    MENU_ITEMS.lock().unwrap().clear();
    FORM_SPECS.lock().unwrap().clear();
    SIDE_SECTIONS.lock().unwrap().clear();
    SIDE_ITEMS.lock().unwrap().clear();
    BOTTOM_FRAGMENTS.lock().unwrap().clear();
    BOTTOM_FRAGMENT_FNS.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn set_lang(value: &str) {
        crate::global_settings::set_raw_option("LANGUAGE", value);
    }

    fn clear_lang() {
        crate::global_settings::remove_raw_option("LANGUAGE");
    }

    fn menu_rows() -> Vec<String> {
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_menu_items(Out::from(&mut buf));
        }
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    fn spec_rows(form_cmd: &str) -> Vec<String> {
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_form_spec(&form_cmd.into(), Out::from(&mut buf));
        }
        ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn register_menu_item_is_idempotent() {
        clear_lang();
        crate::i18n::register_entry("ui-test", "menu.connect", "Connect", "连接");
        crate::i18n::register_entry("ui-test", "menu.group.conn", "Connection", "连接组");
        set_lang("en");
        unsafe {
            let _ = register_menu_item(
                &"menu.connect".into(),
                &"menu.group.conn".into(),
                &"C".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"connect".into(),
                &"".into(),
            );
            let _ = register_menu_item(
                &"menu.connect".into(),
                &"menu.group.conn".into(),
                &"C".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"connect".into(),
                &"".into(),
            );
        }
        let rows = menu_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], "Connect|Connection|C|0|connect|");

        set_lang("zh");
        let rows = menu_rows();
        assert_eq!(rows[0], "连接|连接组|C|0|connect|");

        clear_lang();
        unregister_by_owner("ui-test");
        crate::i18n::unregister_by_owner("ui-test");
    }

    #[test]
    fn form_fields_aggregate_by_cmd() {
        unsafe {
            let _ = register_form_field(&"join_freq".into(), &"field.freq_id".into(), &1, &0, &0);
            let _ = register_form_field(&"join_freq".into(), &"field.password".into(), &0, &1, &0);
        }
        let specs = FORM_SPECS.lock().unwrap();
        let spec = specs.get("join_freq").expect("form spec created");
        assert_eq!(spec.fields.len(), 2);
        assert!(spec.fields[0].required);
        assert!(spec.fields[1].secret);
        // 内部模型保存的是 i18n key，不是译文快照。
        assert_eq!(spec.fields[0].label, "field.freq_id");
    }

    #[test]
    fn form_spec_round_trips_via_out() {
        clear_lang();
        crate::i18n::register_entry("ui-test", "spec.join", "Join Frequency", "加入频道");
        crate::i18n::register_entry("ui-test", "field.freq_id", "Frequency ID:", "频道 ID:");
        set_lang("en");
        unsafe {
            let _ = register_form_spec(&"spec.join".into(), &"join_rt".into(), &"join_rt ".into());
            let _ = register_form_field(&"join_rt".into(), &"field.freq_id".into(), &1, &0, &0);
        }
        let rows = spec_rows("join_rt");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], "SPEC|Join Frequency|join_rt|join_rt ");
        assert_eq!(rows[1], "FIELD|Frequency ID:|1|0|0");

        set_lang("zh");
        let rows = spec_rows("join_rt");
        assert_eq!(rows[0], "SPEC|加入频道|join_rt|join_rt ");
        assert_eq!(rows[1], "FIELD|频道 ID:|1|0|0");

        clear_lang();
        unregister_by_owner("ui-test");
        crate::i18n::unregister_by_owner("ui-test");
    }

    #[test]
    fn side_item_templates_round_trip_via_out() {
        clear_lang();
        crate::i18n::register_entry("ui-test", "side.plugin", "Plugin", "插件");
        set_lang("en");
        unsafe {
            let _ = register_side_section(
                &"sec-rt".into(),
                &"side.plugin".into(),
                &1,
                &"plugins".into(),
            );
            let _ = register_side_item(&"sec-rt".into(), &" • ".into(), &"{name}".into(), &0, &0);
            let _ =
                register_side_item(&"sec-rt".into(), &" : ".into(), &"{namespace}".into(), &0, &1);
        }
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_side_items(Out::from(&mut buf));
        }
        let rows: Vec<String> =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
        // 占位符 key 未收录时原样回传；prefix 符号不译。
        let matched: Vec<&String> = rows.iter().filter(|r| r.starts_with("sec-rt|")).collect();
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0], "sec-rt| • |{name}|0|0");
        assert_eq!(matched[1], "sec-rt| : |{namespace}|0|1");

        set_lang("zh");
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        unsafe {
            let _ = get_side_sections(Out::from(&mut buf));
        }
        let rows: Vec<String> =
            ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
        assert!(rows.iter().any(|r| r == "sec-rt|插件|1|plugins"));

        clear_lang();
        unregister_by_owner("ui-test");
        crate::i18n::unregister_by_owner("ui-test");
    }

    #[test]
    fn unregister_by_owner_removes_owner_ui_and_dict_entries() {
        clear_lang();
        unsafe {
            crate::plugins::set_current_owner(Some("ui-plug-a".to_string()));
            crate::i18n::register_entry("ui-plug-a", "menu.a", "Owner A Menu", "A 菜单");
            let _ = register_menu_item(
                &"menu.a".into(),
                &"menu.g".into(),
                &"".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"connect".into(),
                &"".into(),
            );
            let _ = register_side_section(
                &"sec-owner-a".into(),
                &"side.a".into(),
                &1,
                &"plugins".into(),
            );
            crate::plugins::set_current_owner(Some("ui-plug-b".to_string()));
            crate::i18n::register_entry("ui-plug-b", "menu.b", "Owner B Menu", "B 菜单");
            let _ = register_menu_item(
                &"menu.b".into(),
                &"menu.g".into(),
                &"".into(),
                &cli_core_types::ui::ui_action_kind::RUN,
                &"join".into(),
                &"".into(),
            );
            crate::plugins::set_current_owner(None);
        }

        set_lang("en");
        let rows = menu_rows();
        assert!(rows.iter().any(|r| r.starts_with("Owner A Menu|")));
        assert!(rows.iter().any(|r| r.starts_with("Owner B Menu|")));

        // 对齐 `plugins::unregister_plugin`：UI 注册表与词典表按归属一并清理。
        unregister_by_owner("ui-plug-a");
        crate::i18n::unregister_by_owner("ui-plug-a");
        let rows = menu_rows();
        assert!(!rows.iter().any(|r| r.starts_with("Owner A Menu|")));
        assert!(rows.iter().any(|r| r.starts_with("Owner B Menu|")));
        // 归属插件的词典条目一并移除：未知 key 回退自身。
        assert_eq!(crate::i18n::translate("menu.a"), "menu.a");
        assert_eq!(crate::i18n::translate("menu.b"), "Owner B Menu");

        unregister_by_owner("ui-plug-b");
        crate::i18n::unregister_by_owner("ui-plug-b");
        assert!(!menu_rows().iter().any(|r| r.starts_with("Owner B Menu|")));
        assert_eq!(crate::i18n::translate("menu.b"), "menu.b");

        clear_lang();
    }
}
