// unset.rs
// Unset session variable.
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::option::TaggedOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct UnsetCommand;

impl UnsetCommand {
    /// `key` - Config key name, value_name: KEY
    /// `all` - Clear all session options.
    #[allow(unused_variables)]
    pub fn execute(&self, key: Option<String>, all: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            clear_all_session_options,
            remove_session_option
        );

        if all {
            let _res = clear_all_session_options();
            print_content(&crate::tr("std.prompt.setting.cleared_session").into());
            return Ok(());
        }

        let key = key.ok_or_else(|| {
            anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "key"))
        })?;

        let key_for_display = key.clone();
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        let _res = remove_session_option(&key.into(), Out::from(&mut buf));
        let val = ManuallyDrop::into_inner(buf);
        if let TaggedOption::Some(_) = val {
            print_content(
                &crate::tr("std.prompt.setting.removed_session")
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

impl CommandExecutor for UnsetCommand {
    fn name(&self) -> &'static str {
        "unset"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches.get_one::<String>("key").cloned();
        let all = matches.get_flag("all");
        self.execute(key, all)
    }
}
