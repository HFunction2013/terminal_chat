// unsetg.rs
// Unset global variable.
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct UnsetgCommand;

impl UnsetgCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all global options.
    pub fn execute(&self, key: Option<String>, all: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            clear_all_options,
            remove_global_option
        );

        if all {
            let _res = clear_all_options();
            print_content(&crate::tr("std.prompt.setting.cleared_global").into());
            return Ok(());
        }

        let key = key.ok_or_else(|| {
            anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "key"))
        })?;

        let key_for_display = key.clone();
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let _res = remove_global_option(&key.into(), Out::from(&mut buf));
        let val = ManuallyDrop::into_inner(buf);
        if let TaggedOption::Some(_) = val {
            print_content(
                &crate::tr("std.prompt.setting.removed_global")
                    .replace("{key}", &key_for_display)
                    .into(),
            );
        } else {
            print_content(
                &crate::tr("std.err.setting.key_not_found")
                    .replace("{key}", &key_for_display)
                    .into(),
            );
        }

        Ok(())
    }
}

impl CommandExecutor for UnsetgCommand {
    fn name(&self) -> &'static str {
        "unsetg"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches.get_one::<String>("key").cloned();
        let all = matches.get_flag("all");
        self.execute(key, all)
    }
}
