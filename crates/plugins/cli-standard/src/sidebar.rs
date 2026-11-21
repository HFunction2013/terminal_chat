//! 侧栏栏目壳与条目模板注册表登记（#90）。
//!
//! 侧栏三段（会话 / 成员 / 插件）栏目壳与条目模板登记在 cli-core 注册表：栏目壳
//! 声明标题、默认展开与快照数据源（`source`）；条目模板声明归属栏目、前缀符号、
//! 内容占位（`{name}` / `{namespace}`）、高亮标记与主 / 次行。宿主 TUI 按注册表
//! 模板驱动渲染，实际条目名由宿主快照回填。

use libloading::Library;
use safer_ffi::prelude::*;

/// 侧栏栏目壳定义。
struct SideSectionDef {
    id: &'static str,
    title: &'static str,
    source: &'static str,
    default_open: bool,
}

/// 侧栏条目模板定义。
///
/// `label` 中的 `{name}` / `{namespace}` 为快照字段占位；`highlight` 标记该模板
/// 用于当前高亮条目（如当前会话整行着色）；`line` 0 为主行、1 为次行。
struct SideItemDef {
    section: &'static str,
    prefix: &'static str,
    label: &'static str,
    highlight: bool,
    line: i32,
}

/// 三段侧栏栏目壳（标题 / 默认展开 / 快照数据源一一对应）。
const SIDE_SECTIONS: &[SideSectionDef] = &[
    SideSectionDef { id: "sec-freq", title: "Session", source: "sessions", default_open: true },
    SideSectionDef { id: "sec-members", title: "Members", source: "members", default_open: true },
    SideSectionDef { id: "sec-plugins", title: "Plugin", source: "plugins", default_open: true },
];

/// 侧栏条目模板：会话当前 / 普通两行变体；成员单行；插件主行 + 命名空间次行。
const SIDE_ITEMS: &[SideItemDef] = &[
    // Session（当前会话高亮 `* ` 绿色；普通会话缩进两格）
    SideItemDef { section: "sec-freq", prefix: "* ", label: "{name}", highlight: true, line: 0 },
    SideItemDef { section: "sec-freq", prefix: "  ", label: "{name}", highlight: false, line: 0 },
    // Members
    SideItemDef {
        section: "sec-members",
        prefix: " • ",
        label: "{name}",
        highlight: false,
        line: 0,
    },
    // Plugins（主行插件名，次行命令命名空间）
    SideItemDef {
        section: "sec-plugins",
        prefix: " • ",
        label: "{name}",
        highlight: false,
        line: 0,
    },
    SideItemDef {
        section: "sec-plugins",
        prefix: " : ",
        label: "{namespace}",
        highlight: false,
        line: 1,
    },
];

/// 向 cli-core 注册表登记侧栏栏目壳与条目模板。
pub fn register_sidebar(lib: &Library) {
    cli_core_macros::load_core_symbols!(lib, register_side_section, register_side_item);
    for section in SIDE_SECTIONS {
        let _ = register_side_section(
            &safer_ffi::String::from(section.id),
            &safer_ffi::String::from(section.title),
            &i32::from(section.default_open),
            &safer_ffi::String::from(section.source),
        );
    }
    for item in SIDE_ITEMS {
        let _ = register_side_item(
            &safer_ffi::String::from(item.section),
            &safer_ffi::String::from(item.prefix),
            &safer_ffi::String::from(item.label),
            &i32::from(item.highlight),
            &item.line,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn side_sections_are_well_formed() {
        let mut ids: HashSet<&str> = HashSet::new();
        for section in SIDE_SECTIONS {
            assert!(!section.id.is_empty(), "empty section id");
            assert!(!section.title.is_empty(), "empty section title");
            assert!(!section.source.is_empty(), "empty section source");
            assert!(ids.insert(section.id), "duplicate section id {}", section.id);
        }
    }

    #[test]
    fn side_items_reference_registered_section() {
        let section_ids: HashSet<&str> = SIDE_SECTIONS.iter().map(|s| s.id).collect();
        let mut seen: HashSet<(&str, &str, &str, i32)> = HashSet::new();
        for item in SIDE_ITEMS {
            assert!(
                section_ids.contains(item.section),
                "item references unknown section {}",
                item.section
            );
            assert!(!item.label.is_empty(), "empty item label");
            assert!(item.line == 0 || item.line == 1, "line must be 0 or 1");
            assert!(
                seen.insert((item.section, item.prefix, item.label, item.line)),
                "duplicate side item {}/{}/{}",
                item.section,
                item.prefix,
                item.label
            );
        }
        // 每个栏目至少登记一条主行模板。
        for section in SIDE_SECTIONS {
            assert!(
                SIDE_ITEMS.iter().any(|i| i.section == section.id && i.line == 0),
                "section {} has no primary line template",
                section.id
            );
        }
    }
}
