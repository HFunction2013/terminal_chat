//! 参数表单规格注册表登记（#90，批次 2）。
//!
//! 把宿主内置的 42 个表单规格迁移到 cli-core 注册表：先 `register_form_spec`
//! 登记标题 / 命令 / 前缀，再 `register_form_field` 按命令追加字段。表单字段的
//! 补全类别序号对齐 `cli_core_types::ui::ui_field_kind`。
//!
//! 过渡期这些规格先进注册表；宿主 TUI 仍回退内置硬编码表单，待整体迁移完成后
//! 切换（见 #150）。

use cli_core_types::ui::{ui_action_kind, ui_field_kind as k};
use libloading::Library;
use safer_ffi::prelude::*;

/// 表单字段定义。
struct FormFieldDef {
    label: &'static str,
    required: bool,
    secret: bool,
    kind: i32,
}

/// 表单规格定义：`cmd` 为提交命令名（复合命令含两词，如 `workspace switch`）。
struct FormSpecDef {
    cmd: &'static str,
    title: &'static str,
    prefill: &'static str,
    fields: &'static [FormFieldDef],
}

const PLAIN: i32 = k::PLAIN;
const MEMBER: i32 = k::MEMBER;
const REMOTE_PATH: i32 = k::REMOTE_PATH;
const LOCAL_PATH: i32 = k::LOCAL_PATH;

/// 全部内置表单规格（与宿主 `FORM_SPECS` 一一对应）。
const FORM_SPECS: &[FormSpecDef] = &[
    FormSpecDef {
        cmd: "join_freq",
        title: "Join Frequency",
        prefill: "join_freq ",
        fields: &[
            FormFieldDef { label: "Frequency ID:", required: true, secret: false, kind: PLAIN },
            FormFieldDef {
                label: "Password (optional):",
                required: false,
                secret: true,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "workspace switch",
        title: "Switch Workspace",
        prefill: "workspace switch ",
        fields: &[FormFieldDef {
            label: "Workspace name:",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "workspace new",
        title: "New Workspace",
        prefill: "workspace new ",
        fields: &[FormFieldDef {
            label: "Workspace name (optional):",
            required: false,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "create_freq",
        title: "Create Frequency",
        prefill: "create_freq ",
        fields: &[
            FormFieldDef { label: "Frequency ID:", required: true, secret: false, kind: PLAIN },
            FormFieldDef {
                label: "Password (optional):",
                required: false,
                secret: true,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "msg",
        title: "Send Message",
        prefill: "msg -m ",
        fields: &[
            FormFieldDef {
                label: "Users (optional):",
                required: false,
                secret: false,
                kind: MEMBER,
            },
            FormFieldDef { label: "Message:", required: true, secret: false, kind: PLAIN },
        ],
    },
    FormSpecDef {
        cmd: "reply",
        title: "Reply",
        prefill: "reply ",
        fields: &[FormFieldDef { label: "Message:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "call",
        title: "Call User",
        prefill: "call ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "mute",
        title: "Mute User",
        prefill: "mute ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "unmute",
        title: "Unmute User",
        prefill: "unmute ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "ignore",
        title: "Ignore User",
        prefill: "ignore ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "kick",
        title: "Kick User",
        prefill: "kick ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "ban",
        title: "Ban User",
        prefill: "ban ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "ls",
        title: "List Remote Files",
        prefill: "ls ",
        fields: &[FormFieldDef {
            label: "Directory (optional):",
            required: false,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "upload",
        title: "Upload",
        prefill: "upload ",
        fields: &[
            FormFieldDef { label: "Local path:", required: true, secret: false, kind: LOCAL_PATH },
            FormFieldDef {
                label: "Remote path (optional):",
                required: false,
                secret: false,
                kind: REMOTE_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "download",
        title: "Download",
        prefill: "download ",
        fields: &[
            FormFieldDef {
                label: "Remote path:",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
            FormFieldDef {
                label: "Local path (optional):",
                required: false,
                secret: false,
                kind: LOCAL_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "rm",
        title: "Remove Remote File",
        prefill: "rm ",
        fields: &[FormFieldDef {
            label: "Remote path:",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "mv",
        title: "Move Remote File",
        prefill: "mv ",
        fields: &[
            FormFieldDef { label: "From:", required: true, secret: false, kind: REMOTE_PATH },
            FormFieldDef { label: "To:", required: true, secret: false, kind: REMOTE_PATH },
        ],
    },
    FormSpecDef {
        cmd: "cp",
        title: "Copy Remote File",
        prefill: "cp ",
        fields: &[
            FormFieldDef { label: "From:", required: true, secret: false, kind: REMOTE_PATH },
            FormFieldDef { label: "To:", required: true, secret: false, kind: REMOTE_PATH },
        ],
    },
    FormSpecDef {
        cmd: "load",
        title: "Load Plugin",
        prefill: "load ",
        fields: &[FormFieldDef { label: "Plugin:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "unload",
        title: "Unload Plugin",
        prefill: "unload ",
        fields: &[FormFieldDef {
            label: "Plugin name:",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "shell",
        title: "Local Shell",
        prefill: "shell ",
        fields: &[FormFieldDef { label: "Command:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "unban",
        title: "Unban User",
        prefill: "unban ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "unignore",
        title: "Unignore User",
        prefill: "unignore ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "op",
        title: "Grant Moderator",
        prefill: "op ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "deop",
        title: "Revoke Moderator",
        prefill: "deop ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "trop",
        title: "Transfer Creator",
        prefill: "trop ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "user_info",
        title: "User Info",
        prefill: "user_info ",
        fields: &[FormFieldDef { label: "User:", required: true, secret: false, kind: MEMBER }],
    },
    FormSpecDef {
        cmd: "mkdir",
        title: "Make Remote Directory",
        prefill: "mkdir ",
        fields: &[FormFieldDef {
            label: "Directory:",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "cd",
        title: "Change Remote Directory",
        prefill: "cd ",
        fields: &[FormFieldDef {
            label: "Directory:",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "checksum",
        title: "File Checksum",
        prefill: "checksum ",
        fields: &[FormFieldDef {
            label: "File:",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "file_info",
        title: "File Info",
        prefill: "file_info ",
        fields: &[FormFieldDef {
            label: "File:",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "schedule_msg",
        title: "Schedule Message",
        prefill: "schedule_msg ",
        fields: &[
            FormFieldDef {
                label: "Time (+30s / HH:MM):",
                required: true,
                secret: false,
                kind: PLAIN,
            },
            FormFieldDef { label: "Message:", required: true, secret: false, kind: PLAIN },
            FormFieldDef {
                label: "Users (optional):",
                required: false,
                secret: false,
                kind: MEMBER,
            },
        ],
    },
    FormSpecDef {
        cmd: "cancel",
        title: "Cancel Task",
        prefill: "cancel ",
        fields: &[FormFieldDef { label: "Task ID:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "load namespace",
        title: "Load Namespace",
        prefill: "load namespace ",
        fields: &[FormFieldDef { label: "Namespace:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "unload namespace",
        title: "Unload Namespace",
        prefill: "unload namespace ",
        fields: &[FormFieldDef { label: "Namespace:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "set",
        title: "Set Session Option",
        prefill: "set ",
        fields: &[
            FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN },
            FormFieldDef { label: "Value:", required: true, secret: false, kind: PLAIN },
        ],
    },
    FormSpecDef {
        cmd: "get",
        title: "Get Session Option",
        prefill: "get ",
        fields: &[FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "setg",
        title: "Set Global Option",
        prefill: "setg ",
        fields: &[
            FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN },
            FormFieldDef {
                label: "Value (optional):",
                required: false,
                secret: false,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "getg",
        title: "Get Global Option",
        prefill: "getg ",
        fields: &[FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "unset",
        title: "Unset Session Option",
        prefill: "unset ",
        fields: &[FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "unsetg",
        title: "Unset Global Option",
        prefill: "unsetg ",
        fields: &[FormFieldDef { label: "Key:", required: true, secret: false, kind: PLAIN }],
    },
    FormSpecDef {
        cmd: "freq_info",
        title: "Frequency Info",
        prefill: "freq_info ",
        fields: &[FormFieldDef {
            label: "Frequency (optional):",
            required: false,
            secret: false,
            kind: PLAIN,
        }],
    },
];

/// 向 cli-core 注册表登记全部表单规格及其字段。
pub fn register_forms(lib: &Library) {
    cli_core_macros::load_core_symbols!(lib, register_form_spec, register_form_field);
    for spec in FORM_SPECS {
        let _ = register_form_spec(
            &safer_ffi::String::from(spec.title),
            &safer_ffi::String::from(spec.cmd),
            &safer_ffi::String::from(spec.prefill),
        );
        for field in spec.fields {
            let _ = register_form_field(
                &safer_ffi::String::from(spec.cmd),
                &safer_ffi::String::from(field.label),
                &i32::from(field.required),
                &i32::from(field.secret),
                &field.kind,
            );
        }
    }
}

/// 表单类菜单项：标签 / 分组 / 组助记符 / 关联表单命令名。
pub struct FormEntry {
    pub label: &'static str,
    pub group: &'static str,
    pub accel: &'static str,
    pub form_cmd: &'static str,
}

/// 表单类菜单项（动作类别 FORM）；`form_cmd` 关联 [`FORM_SPECS`] 中的规格。
pub const FORM_ENTRIES: &[FormEntry] = &[
    // Workspace (W)
    FormEntry { label: "Switch...", group: "Workspace", accel: "W", form_cmd: "workspace switch" },
    FormEntry { label: "New...", group: "Workspace", accel: "W", form_cmd: "workspace new" },
    // Frequency (F)
    FormEntry { label: "Join...", group: "Frequency", accel: "F", form_cmd: "join_freq" },
    FormEntry { label: "Create...", group: "Frequency", accel: "F", form_cmd: "create_freq" },
    FormEntry { label: "Info...", group: "Frequency", accel: "F", form_cmd: "freq_info" },
    // Message (M)
    FormEntry { label: "Send...", group: "Message", accel: "M", form_cmd: "msg" },
    FormEntry { label: "Reply...", group: "Message", accel: "M", form_cmd: "reply" },
    FormEntry { label: "Call...", group: "Message", accel: "M", form_cmd: "call" },
    FormEntry { label: "Schedule...", group: "Message", accel: "M", form_cmd: "schedule_msg" },
    FormEntry { label: "Cancel Task...", group: "Message", accel: "M", form_cmd: "cancel" },
    // Moderation (O)
    FormEntry { label: "Mute...", group: "Moderation", accel: "O", form_cmd: "mute" },
    FormEntry { label: "Unmute...", group: "Moderation", accel: "O", form_cmd: "unmute" },
    FormEntry { label: "Ignore...", group: "Moderation", accel: "O", form_cmd: "ignore" },
    FormEntry { label: "Kick...", group: "Moderation", accel: "O", form_cmd: "kick" },
    FormEntry { label: "Ban...", group: "Moderation", accel: "O", form_cmd: "ban" },
    FormEntry { label: "Unban...", group: "Moderation", accel: "O", form_cmd: "unban" },
    FormEntry { label: "Unignore...", group: "Moderation", accel: "O", form_cmd: "unignore" },
    FormEntry { label: "Op...", group: "Moderation", accel: "O", form_cmd: "op" },
    FormEntry { label: "Deop...", group: "Moderation", accel: "O", form_cmd: "deop" },
    FormEntry { label: "Trop...", group: "Moderation", accel: "O", form_cmd: "trop" },
    FormEntry { label: "User Info...", group: "Moderation", accel: "O", form_cmd: "user_info" },
    // Files (L)
    FormEntry { label: "List...", group: "Files", accel: "L", form_cmd: "ls" },
    FormEntry { label: "Upload...", group: "Files", accel: "L", form_cmd: "upload" },
    FormEntry { label: "Download...", group: "Files", accel: "L", form_cmd: "download" },
    FormEntry { label: "Remove...", group: "Files", accel: "L", form_cmd: "rm" },
    FormEntry { label: "Move...", group: "Files", accel: "L", form_cmd: "mv" },
    FormEntry { label: "Copy...", group: "Files", accel: "L", form_cmd: "cp" },
    FormEntry { label: "Mkdir...", group: "Files", accel: "L", form_cmd: "mkdir" },
    FormEntry { label: "Cd...", group: "Files", accel: "L", form_cmd: "cd" },
    FormEntry { label: "Checksum...", group: "Files", accel: "L", form_cmd: "checksum" },
    FormEntry { label: "File Info...", group: "Files", accel: "L", form_cmd: "file_info" },
    // Plugins (P)
    FormEntry { label: "Load...", group: "Plugins", accel: "P", form_cmd: "load" },
    FormEntry { label: "Unload...", group: "Plugins", accel: "P", form_cmd: "unload" },
    FormEntry {
        label: "Load Namespace...",
        group: "Plugins",
        accel: "P",
        form_cmd: "load namespace",
    },
    FormEntry {
        label: "Unload Namespace...",
        group: "Plugins",
        accel: "P",
        form_cmd: "unload namespace",
    },
    FormEntry { label: "Shell...", group: "Plugins", accel: "P", form_cmd: "shell" },
    // Settings (S)
    FormEntry { label: "Set...", group: "Settings", accel: "S", form_cmd: "set" },
    FormEntry { label: "Get...", group: "Settings", accel: "S", form_cmd: "get" },
    FormEntry { label: "Set Global...", group: "Settings", accel: "S", form_cmd: "setg" },
    FormEntry { label: "Get Global...", group: "Settings", accel: "S", form_cmd: "getg" },
    FormEntry { label: "Unset...", group: "Settings", accel: "S", form_cmd: "unset" },
    FormEntry { label: "Unset Global...", group: "Settings", accel: "S", form_cmd: "unsetg" },
];

/// 登记表单类菜单项（动作类别 FORM，关联 `form_cmd`）。
pub fn register_form_menu_items(lib: &Library) {
    cli_core_macros::load_core_symbols!(lib, register_menu_item);
    for entry in FORM_ENTRIES {
        let _ = register_menu_item(
            &safer_ffi::String::from(entry.label),
            &safer_ffi::String::from(entry.group),
            &safer_ffi::String::from(entry.accel),
            &ui_action_kind::FORM,
            &safer_ffi::String::from(""),
            &safer_ffi::String::from(entry.form_cmd),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn form_specs_are_well_formed() {
        let mut cmds: HashSet<&str> = HashSet::new();
        for spec in FORM_SPECS {
            assert!(!spec.cmd.is_empty(), "empty cmd");
            assert!(!spec.title.is_empty(), "empty title for {}", spec.cmd);
            assert!(!spec.prefill.is_empty(), "empty prefill for {}", spec.cmd);
            assert!(!spec.fields.is_empty(), "form {} has no fields", spec.cmd);
            assert!(cmds.insert(spec.cmd), "duplicate form cmd {}", spec.cmd);
            for field in spec.fields {
                assert!(!field.label.is_empty(), "empty label in {}", spec.cmd);
            }
        }
    }

    #[test]
    fn form_menu_entries_reference_registered_spec() {
        let spec_cmds: HashSet<&str> = FORM_SPECS.iter().map(|s| s.cmd).collect();
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for entry in FORM_ENTRIES {
            assert!(
                spec_cmds.contains(entry.form_cmd),
                "menu item {} references missing form cmd {}",
                entry.label,
                entry.form_cmd
            );
            assert!(
                seen.insert((entry.group, entry.label)),
                "duplicate form menu item {}/{}",
                entry.group,
                entry.label
            );
        }
    }
}
