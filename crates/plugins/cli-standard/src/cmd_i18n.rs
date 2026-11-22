//! Post-build localization of the generated clap command tree.
//!
//! `command.rs` (generated from `commands.yaml` by `yaml2cmd_build`) embeds
//! English `about` / `help` literals. Once the tree is built we walk it once
//! and replace command `about` and argument `help` strings with dictionary
//! translations resolved through the core `lookup_i18n` hook.
//!
//! Only description fields are touched. clap's single-field builder setters
//! preserve every other attribute: subcommands, aliases, short/long flags,
//! required flags, actions, default values, `num_args`, `conflicts_with` and
//! hidden flags stay exactly as generated.
//!
//! `value_name` placeholders (`KEY`, `FREQ_ID`, …) are usage-line identifiers,
//! not prose; they stay English by design and have no dictionary keys.

use clap::Command;

use crate::tr;

/// Look up a dictionary key; `None` when the core dictionary does not know
/// it (the plugin `tr()` echoes the key itself for unknown entries).
fn lookup(key: &str) -> Option<String> {
    let translated = tr(key);
    if translated == key { None } else { Some(translated) }
}

/// Walk the command tree rooted at `cmd`, localizing `about` and `help`.
/// `path` are the command segments below the plugin root (e.g.
/// `["create", "alias"]`) used to compose dictionary keys.
fn localize(mut cmd: Command, path: &[String]) -> Command {
    if !path.is_empty() {
        let about_key = format!("std.help.{}.about", path.join("."));
        if let Some(about) = lookup(&about_key) {
            cmd = cmd.about(about);
        }
    }

    cmd = cmd.mut_args(|arg| {
        let arg_id = arg.get_id().as_str();
        let help_key = format!("std.help.{}.{}.help", path.join("."), arg_id);
        match lookup(&help_key) {
            Some(help) => arg.help(help),
            None => arg,
        }
    });

    cmd.mut_subcommands(|sub| {
        let mut child_path = path.to_vec();
        child_path.push(sub.get_name().to_string());
        localize(sub, &child_path)
    })
}

/// Entry point: localize the built plugin command tree in place.
pub(crate) fn localize_commands(cmd: Command) -> Command {
    localize(cmd, &[])
}
