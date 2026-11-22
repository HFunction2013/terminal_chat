//! 菜单注册表登记（#90）。
//!
//! 本模块把可被插件调度的命令菜单条目登记进 cli-core 注册表；宿主 TUI 按注册表
//! 驱动渲染。内置退出 / 信息弹窗命令（exit / about / copyright / license /
//! help 等）由宿主本地分发，不在此登记。
//!
//! 表内 `label` / `group` 存的是 i18n key；注册时不固化译文快照，查询时由 core
//! 按当前语言现译。`accel` / `argv` 等非译字段原样登记。

use cli_core_types::ui::ui_action_kind;
use libloading::Library;
use safer_ffi::prelude::*;

/// 一条 Run 类菜单项：标签 key / 分组 key / 组助记符 / 命令行参数（空格分隔）。
struct RunEntry {
    label: &'static str,
    group: &'static str,
    accel: &'static str,
    argv: &'static str,
}

/// 由插件 dispatch 的 Run 类菜单项；表单类菜单项随批次 2 迁移。
const RUN_ENTRIES: &[RunEntry] = &[
    // Connection (C)
    RunEntry {
        label: "std.menu.connection.connect",
        group: "std.group.connection",
        accel: "C",
        argv: "connect",
    },
    RunEntry {
        label: "std.menu.connection.reconnect",
        group: "std.group.connection",
        accel: "C",
        argv: "reconnect",
    },
    RunEntry {
        label: "std.menu.connection.disconnect",
        group: "std.group.connection",
        accel: "C",
        argv: "disconnect",
    },
    RunEntry {
        label: "std.menu.connection.whoami",
        group: "std.group.connection",
        accel: "C",
        argv: "whoami",
    },
    RunEntry {
        label: "std.menu.connection.status",
        group: "std.group.connection",
        accel: "C",
        argv: "status",
    },
    RunEntry {
        label: "std.menu.connection.route",
        group: "std.group.connection",
        accel: "C",
        argv: "route",
    },
    // Workspace (W)
    RunEntry {
        label: "std.menu.workspace.list",
        group: "std.group.workspace",
        accel: "W",
        argv: "workspace list",
    },
    RunEntry {
        label: "std.menu.workspace.show",
        group: "std.group.workspace",
        accel: "W",
        argv: "workspace show",
    },
    // Frequency (F)
    RunEntry {
        label: "std.menu.frequency.leave",
        group: "std.group.frequency",
        accel: "F",
        argv: "leave_freq",
    },
    RunEntry {
        label: "std.menu.frequency.background",
        group: "std.group.frequency",
        accel: "F",
        argv: "background",
    },
    RunEntry {
        label: "std.menu.frequency.foreground",
        group: "std.group.frequency",
        accel: "F",
        argv: "foreground",
    },
    RunEntry {
        label: "std.menu.frequency.away",
        group: "std.group.frequency",
        accel: "F",
        argv: "away",
    },
    RunEntry {
        label: "std.menu.frequency.notifications",
        group: "std.group.frequency",
        accel: "F",
        argv: "notifications",
    },
    RunEntry {
        label: "std.menu.frequency.sessions",
        group: "std.group.frequency",
        accel: "F",
        argv: "sessions -l",
    },
    RunEntry {
        label: "std.menu.frequency.disband",
        group: "std.group.frequency",
        accel: "F",
        argv: "disband",
    },
    RunEntry {
        label: "std.menu.frequency.active_freqs",
        group: "std.group.frequency",
        accel: "F",
        argv: "show active_freqs",
    },
    RunEntry {
        label: "std.menu.frequency.active_users",
        group: "std.group.frequency",
        accel: "F",
        argv: "show active_users",
    },
    // Files (L)
    RunEntry { label: "std.menu.files.tree", group: "std.group.files", accel: "L", argv: "tree" },
    RunEntry { label: "std.menu.files.pwd", group: "std.group.files", accel: "L", argv: "pwd" },
    RunEntry { label: "std.menu.files.lpwd", group: "std.group.files", accel: "L", argv: "lpwd" },
    RunEntry { label: "std.menu.files.lls", group: "std.group.files", accel: "L", argv: "lls" },
    RunEntry { label: "std.menu.files.lcd", group: "std.group.files", accel: "L", argv: "lcd" },
    // Plugins (P)
    RunEntry {
        label: "std.menu.plugins.show",
        group: "std.group.plugins",
        accel: "P",
        argv: "show plugins",
    },
    // Settings (S)
    RunEntry {
        label: "std.menu.settings.options",
        group: "std.group.settings",
        accel: "S",
        argv: "show options",
    },
    RunEntry {
        label: "std.menu.settings.locals",
        group: "std.group.settings",
        accel: "S",
        argv: "show locals",
    },
];

/// 向 cli-core 注册表登记本插件的 Run 类菜单项。
pub fn register_menu(lib: &Library) {
    cli_core_macros::load_core_symbols!(lib, register_menu_item);
    for entry in RUN_ENTRIES {
        let _ = register_menu_item(
            &safer_ffi::String::from(entry.label),
            &safer_ffi::String::from(entry.group),
            &safer_ffi::String::from(entry.accel),
            &ui_action_kind::RUN,
            &safer_ffi::String::from(entry.argv),
            &safer_ffi::String::from(""),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn run_entries_are_well_formed() {
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for entry in RUN_ENTRIES {
            assert!(!entry.label.is_empty(), "empty label");
            assert!(!entry.group.is_empty(), "empty group");
            assert!(!entry.accel.is_empty(), "empty accel");
            assert!(!entry.argv.is_empty(), "empty argv");
            assert!(
                seen.insert((entry.group, entry.label)),
                "duplicate menu item {}/{}",
                entry.group,
                entry.label
            );
        }
    }
}
