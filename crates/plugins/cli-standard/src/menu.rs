//! 菜单注册表登记（#90）。
//!
//! 本模块把可被插件调度的命令菜单条目登记进 cli-core 注册表；宿主 TUI 按注册表
//! 驱动渲染菜单。内置退出 / 信息弹窗命令（exit / about / copyright / license /
//! help 等）由宿主本地分发，不在此登记。

use cli_core_types::ui::ui_action_kind;
use libloading::Library;
use safer_ffi::prelude::*;

/// 一条 Run 类菜单项：标签 / 分组 / 组助记符 / 命令行参数（空格分隔）。
struct RunEntry {
    label: &'static str,
    group: &'static str,
    accel: &'static str,
    argv: &'static str,
}

/// 由插件 dispatch 的 Run 类菜单项；表单类菜单项随批次 2 迁移。
const RUN_ENTRIES: &[RunEntry] = &[
    // Connection (C)
    RunEntry { label: "Connect", group: "Connection", accel: "C", argv: "connect" },
    RunEntry { label: "Reconnect", group: "Connection", accel: "C", argv: "reconnect" },
    RunEntry { label: "Disconnect", group: "Connection", accel: "C", argv: "disconnect" },
    RunEntry { label: "Whoami", group: "Connection", accel: "C", argv: "whoami" },
    RunEntry { label: "Status", group: "Connection", accel: "C", argv: "status" },
    RunEntry { label: "Route", group: "Connection", accel: "C", argv: "route" },
    // Workspace (W)
    RunEntry { label: "List", group: "Workspace", accel: "W", argv: "workspace list" },
    RunEntry { label: "Show", group: "Workspace", accel: "W", argv: "workspace show" },
    // Frequency (F)
    RunEntry { label: "Leave", group: "Frequency", accel: "F", argv: "leave_freq" },
    RunEntry { label: "Background", group: "Frequency", accel: "F", argv: "background" },
    RunEntry { label: "Foreground", group: "Frequency", accel: "F", argv: "foreground" },
    RunEntry { label: "Away", group: "Frequency", accel: "F", argv: "away" },
    RunEntry { label: "Notifications", group: "Frequency", accel: "F", argv: "notifications" },
    RunEntry { label: "Sessions", group: "Frequency", accel: "F", argv: "sessions -l" },
    RunEntry { label: "Disband", group: "Frequency", accel: "F", argv: "disband" },
    RunEntry { label: "Active freqs", group: "Frequency", accel: "F", argv: "show active_freqs" },
    RunEntry { label: "Active users", group: "Frequency", accel: "F", argv: "show active_users" },
    // Files (L)
    RunEntry { label: "Tree", group: "Files", accel: "L", argv: "tree" },
    RunEntry { label: "Pwd (remote)", group: "Files", accel: "L", argv: "pwd" },
    RunEntry { label: "Local Pwd", group: "Files", accel: "L", argv: "lpwd" },
    RunEntry { label: "Local List", group: "Files", accel: "L", argv: "lls" },
    RunEntry { label: "Local Cd...", group: "Files", accel: "L", argv: "lcd" },
    // Plugins (P)
    RunEntry { label: "Show", group: "Plugins", accel: "P", argv: "show plugins" },
    // Settings (S)
    RunEntry { label: "Options", group: "Settings", accel: "S", argv: "show options" },
    RunEntry { label: "Locals", group: "Settings", accel: "S", argv: "show locals" },
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
