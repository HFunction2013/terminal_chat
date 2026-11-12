// options.rs
// Show available set/setg options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct OptionsCommand;

impl OptionsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            get_registered_options,
            get_all_options
        );

        // 已注册、可接受的选项 key（由插件在 init 阶段登记）。
        let mut reg_buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_registered_options(Out::from(&mut reg_buf));
        let registered = ManuallyDrop::into_inner(reg_buf);

        print_content(&"Registered options (set/setg):".into());
        for key in registered.iter() {
            print_content(&format!("  {key}").into());
        }

        // 动态列出当前已设置的全局选项，与实际状态同步。
        let mut set_buf: ManuallyDrop<safer_ffi::vec::Vec<GlobalOption>> =
            ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
        let _res = get_all_options(Out::from(&mut set_buf));
        let set = ManuallyDrop::into_inner(set_buf);
        if !set.is_empty() {
            print_content(&"Currently set (setg):".into());
            for o in set.iter() {
                print_content(&format!("  {} = {}", o.key, o.value).into());
            }
        }
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
