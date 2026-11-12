// options.rs
// Show available set/setg options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

/// 当前实际使用的 set/setg 配置项（key 与说明），与代码读取处保持同步。
const KNOWN_OPTIONS: &[(&str, &str)] = &[
    ("SERVER", "server address to connect (connect)"),
    ("PORT", "server port (connect)"),
    ("USERNAME", "your display username (connect, required)"),
    ("QUIET", "mute all incoming notifications (on/true/1/yes)"),
];

pub struct OptionsCommand;

impl OptionsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, get_all_options);

        print_content(&"Built-in options (setg):".into());
        for (key, desc) in KNOWN_OPTIONS {
            print_content(&format!("  {key:<12} {desc}").into());
        }
        // 动态列出当前已设置的全局选项，与实际状态同步。
        let mut buf: ManuallyDrop<safer_ffi::vec::Vec<GlobalOption>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_options(Out::from(&mut buf));
        let set = ManuallyDrop::into_inner(buf);
        if !set.is_empty() {
            print_content(&"Currently set (setg):".into());
            for o in set.iter() {
                print_content(&format!("  {} = {}", o.key.to_string(), o.value.to_string()).into());
            }
        }
        print_content(&"  (any custom key is also accepted via `set`/`setg`)".into());
        Ok(())
    }
}

impl CommandExecutor for OptionsCommand {
    fn name(&self) -> &'static str {
        "options"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
