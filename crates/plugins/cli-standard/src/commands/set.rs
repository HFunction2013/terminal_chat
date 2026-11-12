// set.rs
// Set session options
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct SetCommand;

impl SetCommand {
    /// `key` - Session config key, required, value_name: KEY
    /// `value` - Session config value, required, value_name: VALUE
    #[allow(unused_variables)]
    pub fn execute(&self, key: String, value: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            set_session_option,
            option_is_registered
        );

        if option_is_registered(&key.clone().into()).code != 0 {
            print_content(
                &format!("This option has not been registered, use `unset(g) {key}` to unset it.")
                    .into(),
            );
            return Ok(());
        }

        let option = GlobalOption::new(&key, &value);
        let mut echo: ManuallyDrop<GlobalOption> = ManuallyDrop::new(GlobalOption::default());
        let _res = set_session_option(&option, Out::from(&mut echo));
        let _echo = ManuallyDrop::into_inner(echo);
        print_content(&format!("{key} => {value}").into());
        Ok(())
    }
}

impl CommandExecutor for SetCommand {
    fn name(&self) -> &'static str {
        "set"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        let value = matches
            .get_one::<String>("value")
            .ok_or_else(|| anyhow!("Missing required argument: value"))?
            .clone();
        self.execute(key, value)
    }
}
