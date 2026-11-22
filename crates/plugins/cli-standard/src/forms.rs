//! 参数表单规格注册表登记（#90）。
//!
//! 42 个表单规格登记在 cli-core 注册表：先 `register_form_spec` 登记标题 / 命令 /
//! 前缀，再 `register_form_field` 按命令追加字段。表单字段的补全类别序号对齐
//! `cli_core_types::ui::ui_field_kind`；宿主 TUI 按注册表规格渲染表单。
//!
//! 表内标题 / 字段标签存的是 i18n key；注册时不固化译文快照，查询时由 core 按
//! 当前语言现译。`cmd` / `prefill` / `required` / `secret` / `kind` 等非译字段
//! 原样登记。

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
const OPTION_KEY: i32 = k::OPTION_KEY;

/// 全部内置表单规格（与宿主 `FORM_SPECS` 一一对应）。
const FORM_SPECS: &[FormSpecDef] = &[
    FormSpecDef {
        cmd: "join_freq",
        title: "std.form.join_freq.title",
        prefill: "join_freq ",
        fields: &[
            FormFieldDef {
                label: "std.field.join_freq.freq_id",
                required: true,
                secret: false,
                kind: PLAIN,
            },
            FormFieldDef {
                label: "std.field.join_freq.password",
                required: false,
                secret: true,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "workspace switch",
        title: "std.form.workspace_switch.title",
        prefill: "workspace switch ",
        fields: &[FormFieldDef {
            label: "std.field.workspace_switch.name",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "workspace new",
        title: "std.form.workspace_new.title",
        prefill: "workspace new ",
        fields: &[FormFieldDef {
            label: "std.field.workspace_new.name",
            required: false,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "create_freq",
        title: "std.form.create_freq.title",
        prefill: "create_freq ",
        fields: &[
            FormFieldDef {
                label: "std.field.create_freq.freq_id",
                required: true,
                secret: false,
                kind: PLAIN,
            },
            FormFieldDef {
                label: "std.field.create_freq.password",
                required: false,
                secret: true,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "msg",
        title: "std.form.msg.title",
        prefill: "msg -m ",
        fields: &[
            FormFieldDef {
                label: "std.field.msg.users",
                required: false,
                secret: false,
                kind: MEMBER,
            },
            FormFieldDef {
                label: "std.field.msg.message",
                required: true,
                secret: false,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "reply",
        title: "std.form.reply.title",
        prefill: "reply ",
        fields: &[FormFieldDef {
            label: "std.field.reply.message",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "call",
        title: "std.form.call.title",
        prefill: "call ",
        fields: &[FormFieldDef {
            label: "std.field.call.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "mute",
        title: "std.form.mute.title",
        prefill: "mute ",
        fields: &[FormFieldDef {
            label: "std.field.mute.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "unmute",
        title: "std.form.unmute.title",
        prefill: "unmute ",
        fields: &[FormFieldDef {
            label: "std.field.unmute.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "ignore",
        title: "std.form.ignore.title",
        prefill: "ignore ",
        fields: &[FormFieldDef {
            label: "std.field.ignore.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "kick",
        title: "std.form.kick.title",
        prefill: "kick ",
        fields: &[FormFieldDef {
            label: "std.field.kick.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "ban",
        title: "std.form.ban.title",
        prefill: "ban ",
        fields: &[FormFieldDef {
            label: "std.field.ban.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "ls",
        title: "std.form.ls.title",
        prefill: "ls ",
        fields: &[FormFieldDef {
            label: "std.field.ls.dir",
            required: false,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "upload",
        title: "std.form.upload.title",
        prefill: "upload ",
        fields: &[
            FormFieldDef {
                label: "std.field.upload.local_path",
                required: true,
                secret: false,
                kind: LOCAL_PATH,
            },
            FormFieldDef {
                label: "std.field.upload.remote_path",
                required: false,
                secret: false,
                kind: REMOTE_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "download",
        title: "std.form.download.title",
        prefill: "download ",
        fields: &[
            FormFieldDef {
                label: "std.field.download.remote_path",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
            FormFieldDef {
                label: "std.field.download.local_path",
                required: false,
                secret: false,
                kind: LOCAL_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "rm",
        title: "std.form.rm.title",
        prefill: "rm ",
        fields: &[FormFieldDef {
            label: "std.field.rm.remote_path",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "mv",
        title: "std.form.mv.title",
        prefill: "mv ",
        fields: &[
            FormFieldDef {
                label: "std.field.mv.from",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
            FormFieldDef {
                label: "std.field.mv.to",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "cp",
        title: "std.form.cp.title",
        prefill: "cp ",
        fields: &[
            FormFieldDef {
                label: "std.field.cp.from",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
            FormFieldDef {
                label: "std.field.cp.to",
                required: true,
                secret: false,
                kind: REMOTE_PATH,
            },
        ],
    },
    FormSpecDef {
        cmd: "load",
        title: "std.form.load.title",
        prefill: "load ",
        fields: &[FormFieldDef {
            label: "std.field.load.plugin",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "unload",
        title: "std.form.unload.title",
        prefill: "unload ",
        fields: &[FormFieldDef {
            label: "std.field.unload.plugin",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "shell",
        title: "std.form.shell.title",
        prefill: "shell ",
        fields: &[FormFieldDef {
            label: "std.field.shell.command",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "unban",
        title: "std.form.unban.title",
        prefill: "unban ",
        fields: &[FormFieldDef {
            label: "std.field.unban.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "unignore",
        title: "std.form.unignore.title",
        prefill: "unignore ",
        fields: &[FormFieldDef {
            label: "std.field.unignore.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "op",
        title: "std.form.op.title",
        prefill: "op ",
        fields: &[FormFieldDef {
            label: "std.field.op.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "deop",
        title: "std.form.deop.title",
        prefill: "deop ",
        fields: &[FormFieldDef {
            label: "std.field.deop.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "trop",
        title: "std.form.trop.title",
        prefill: "trop ",
        fields: &[FormFieldDef {
            label: "std.field.trop.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "user_info",
        title: "std.form.user_info.title",
        prefill: "user_info ",
        fields: &[FormFieldDef {
            label: "std.field.user_info.user",
            required: true,
            secret: false,
            kind: MEMBER,
        }],
    },
    FormSpecDef {
        cmd: "mkdir",
        title: "std.form.mkdir.title",
        prefill: "mkdir ",
        fields: &[FormFieldDef {
            label: "std.field.mkdir.dir",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "cd",
        title: "std.form.cd.title",
        prefill: "cd ",
        fields: &[FormFieldDef {
            label: "std.field.cd.dir",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "checksum",
        title: "std.form.checksum.title",
        prefill: "checksum ",
        fields: &[FormFieldDef {
            label: "std.field.checksum.file",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "file_info",
        title: "std.form.file_info.title",
        prefill: "file_info ",
        fields: &[FormFieldDef {
            label: "std.field.file_info.file",
            required: true,
            secret: false,
            kind: REMOTE_PATH,
        }],
    },
    FormSpecDef {
        cmd: "schedule_msg",
        title: "std.form.schedule_msg.title",
        prefill: "schedule_msg ",
        fields: &[
            FormFieldDef {
                label: "std.field.schedule_msg.time",
                required: true,
                secret: false,
                kind: PLAIN,
            },
            FormFieldDef {
                label: "std.field.schedule_msg.message",
                required: true,
                secret: false,
                kind: PLAIN,
            },
            FormFieldDef {
                label: "std.field.schedule_msg.users",
                required: false,
                secret: false,
                kind: MEMBER,
            },
        ],
    },
    FormSpecDef {
        cmd: "cancel",
        title: "std.form.cancel.title",
        prefill: "cancel ",
        fields: &[FormFieldDef {
            label: "std.field.cancel.task_id",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "load namespace",
        title: "std.form.load_namespace.title",
        prefill: "load namespace ",
        fields: &[FormFieldDef {
            label: "std.field.load_namespace.namespace",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "unload namespace",
        title: "std.form.unload_namespace.title",
        prefill: "unload namespace ",
        fields: &[FormFieldDef {
            label: "std.field.unload_namespace.namespace",
            required: true,
            secret: false,
            kind: PLAIN,
        }],
    },
    FormSpecDef {
        cmd: "set",
        title: "std.form.set.title",
        prefill: "set ",
        fields: &[
            FormFieldDef {
                label: "std.field.set.key",
                required: true,
                secret: false,
                kind: OPTION_KEY,
            },
            FormFieldDef {
                label: "std.field.set.value",
                required: true,
                secret: false,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "get",
        title: "std.form.get.title",
        prefill: "get ",
        fields: &[FormFieldDef {
            label: "std.field.get.key",
            required: true,
            secret: false,
            kind: OPTION_KEY,
        }],
    },
    FormSpecDef {
        cmd: "setg",
        title: "std.form.setg.title",
        prefill: "setg ",
        fields: &[
            FormFieldDef {
                label: "std.field.setg.key",
                required: true,
                secret: false,
                kind: OPTION_KEY,
            },
            FormFieldDef {
                label: "std.field.setg.value",
                required: false,
                secret: false,
                kind: PLAIN,
            },
        ],
    },
    FormSpecDef {
        cmd: "getg",
        title: "std.form.getg.title",
        prefill: "getg ",
        fields: &[FormFieldDef {
            label: "std.field.getg.key",
            required: true,
            secret: false,
            kind: OPTION_KEY,
        }],
    },
    FormSpecDef {
        cmd: "unset",
        title: "std.form.unset.title",
        prefill: "unset ",
        fields: &[FormFieldDef {
            label: "std.field.unset.key",
            required: true,
            secret: false,
            kind: OPTION_KEY,
        }],
    },
    FormSpecDef {
        cmd: "unsetg",
        title: "std.form.unsetg.title",
        prefill: "unsetg ",
        fields: &[FormFieldDef {
            label: "std.field.unsetg.key",
            required: true,
            secret: false,
            kind: OPTION_KEY,
        }],
    },
    FormSpecDef {
        cmd: "freq_info",
        title: "std.form.freq_info.title",
        prefill: "freq_info ",
        fields: &[FormFieldDef {
            label: "std.field.freq_info.freq",
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

/// 表单类菜单项：标签 key / 分组 key / 组助记符 / 关联表单命令名。
pub struct FormEntry {
    pub label: &'static str,
    pub group: &'static str,
    pub accel: &'static str,
    pub form_cmd: &'static str,
}

/// 表单类菜单项（动作类别 FORM）；`form_cmd` 关联 [`FORM_SPECS`] 中的规格。
pub const FORM_ENTRIES: &[FormEntry] = &[
    // Workspace (W)
    FormEntry {
        label: "std.menu.workspace.switch",
        group: "std.group.workspace",
        accel: "W",
        form_cmd: "workspace switch",
    },
    FormEntry {
        label: "std.menu.workspace.new",
        group: "std.group.workspace",
        accel: "W",
        form_cmd: "workspace new",
    },
    // Frequency (F)
    FormEntry {
        label: "std.menu.frequency.join",
        group: "std.group.frequency",
        accel: "F",
        form_cmd: "join_freq",
    },
    FormEntry {
        label: "std.menu.frequency.create",
        group: "std.group.frequency",
        accel: "F",
        form_cmd: "create_freq",
    },
    FormEntry {
        label: "std.menu.frequency.info",
        group: "std.group.frequency",
        accel: "F",
        form_cmd: "freq_info",
    },
    // Message (M)
    FormEntry {
        label: "std.menu.message.send",
        group: "std.group.message",
        accel: "M",
        form_cmd: "msg",
    },
    FormEntry {
        label: "std.menu.message.reply",
        group: "std.group.message",
        accel: "M",
        form_cmd: "reply",
    },
    FormEntry {
        label: "std.menu.message.call",
        group: "std.group.message",
        accel: "M",
        form_cmd: "call",
    },
    FormEntry {
        label: "std.menu.message.schedule",
        group: "std.group.message",
        accel: "M",
        form_cmd: "schedule_msg",
    },
    FormEntry {
        label: "std.menu.message.cancel",
        group: "std.group.message",
        accel: "M",
        form_cmd: "cancel",
    },
    // Moderation (O)
    FormEntry {
        label: "std.menu.moderation.mute",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "mute",
    },
    FormEntry {
        label: "std.menu.moderation.unmute",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "unmute",
    },
    FormEntry {
        label: "std.menu.moderation.ignore",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "ignore",
    },
    FormEntry {
        label: "std.menu.moderation.kick",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "kick",
    },
    FormEntry {
        label: "std.menu.moderation.ban",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "ban",
    },
    FormEntry {
        label: "std.menu.moderation.unban",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "unban",
    },
    FormEntry {
        label: "std.menu.moderation.unignore",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "unignore",
    },
    FormEntry {
        label: "std.menu.moderation.op",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "op",
    },
    FormEntry {
        label: "std.menu.moderation.deop",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "deop",
    },
    FormEntry {
        label: "std.menu.moderation.trop",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "trop",
    },
    FormEntry {
        label: "std.menu.moderation.user_info",
        group: "std.group.moderation",
        accel: "O",
        form_cmd: "user_info",
    },
    // Files (L)
    FormEntry {
        label: "std.menu.files.list",
        group: "std.group.files",
        accel: "L",
        form_cmd: "ls",
    },
    FormEntry {
        label: "std.menu.files.upload",
        group: "std.group.files",
        accel: "L",
        form_cmd: "upload",
    },
    FormEntry {
        label: "std.menu.files.download",
        group: "std.group.files",
        accel: "L",
        form_cmd: "download",
    },
    FormEntry {
        label: "std.menu.files.remove",
        group: "std.group.files",
        accel: "L",
        form_cmd: "rm",
    },
    FormEntry {
        label: "std.menu.files.move",
        group: "std.group.files",
        accel: "L",
        form_cmd: "mv",
    },
    FormEntry {
        label: "std.menu.files.copy",
        group: "std.group.files",
        accel: "L",
        form_cmd: "cp",
    },
    FormEntry {
        label: "std.menu.files.mkdir",
        group: "std.group.files",
        accel: "L",
        form_cmd: "mkdir",
    },
    FormEntry { label: "std.menu.files.cd", group: "std.group.files", accel: "L", form_cmd: "cd" },
    FormEntry {
        label: "std.menu.files.checksum",
        group: "std.group.files",
        accel: "L",
        form_cmd: "checksum",
    },
    FormEntry {
        label: "std.menu.files.file_info",
        group: "std.group.files",
        accel: "L",
        form_cmd: "file_info",
    },
    // Plugins (P)
    FormEntry {
        label: "std.menu.plugins.load",
        group: "std.group.plugins",
        accel: "P",
        form_cmd: "load",
    },
    FormEntry {
        label: "std.menu.plugins.unload",
        group: "std.group.plugins",
        accel: "P",
        form_cmd: "unload",
    },
    FormEntry {
        label: "std.menu.plugins.load_namespace",
        group: "std.group.plugins",
        accel: "P",
        form_cmd: "load namespace",
    },
    FormEntry {
        label: "std.menu.plugins.unload_namespace",
        group: "std.group.plugins",
        accel: "P",
        form_cmd: "unload namespace",
    },
    FormEntry {
        label: "std.menu.plugins.shell",
        group: "std.group.plugins",
        accel: "P",
        form_cmd: "shell",
    },
    // Settings (S)
    FormEntry {
        label: "std.menu.settings.set",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "set",
    },
    FormEntry {
        label: "std.menu.settings.get",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "get",
    },
    FormEntry {
        label: "std.menu.settings.setg",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "setg",
    },
    FormEntry {
        label: "std.menu.settings.getg",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "getg",
    },
    FormEntry {
        label: "std.menu.settings.unset",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "unset",
    },
    FormEntry {
        label: "std.menu.settings.unsetg",
        group: "std.group.settings",
        accel: "S",
        form_cmd: "unsetg",
    },
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

    /// 选项类表单的 `Key:` 字段必须声明为 `OPTION_KEY`，与 `commands.yaml`
    /// 中同名命令的参数类型对齐；标成 `PLAIN` 会让宿主弹窗内不补全选项 key。
    #[test]
    fn option_key_forms_mark_key_field_as_option_key() {
        let option_cmds = ["set", "get", "setg", "getg", "unset", "unsetg"];
        for spec in FORM_SPECS {
            if option_cmds.contains(&spec.cmd) {
                let key_field = &spec.fields[0];
                assert_eq!(
                    key_field.kind,
                    k::OPTION_KEY,
                    "{} key field must be OPTION_KEY",
                    spec.cmd
                );
            }
        }
    }
}
