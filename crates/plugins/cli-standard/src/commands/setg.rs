// setg.rs
// Set global options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core_types::GlobalOption;
use safer_ffi::prelude::*;
use std::mem::ManuallyDrop;

pub struct SetgCommand;

impl SetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    /// `value` - Target config value, value_name: VALUE
    /// `password` - use rpassword to read the value, conflicts with: value
    pub fn execute(&self, key: String, value: Option<String>, password: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(
            lib,
            print_content,
            set_global_option,
            option_is_registered
        );

        if option_is_registered(&key.clone().into()).code != 0 {
            print_content(
                &format!("This option has not been registered, use `unset(g) {key}` to unset it.")
                    .into(),
            );
            return Ok(());
        }

        let value = if password {
            rpassword::prompt_password(format!("Enter value for '{key}': "))?.trim().to_string()
        } else {
            value
            .ok_or_else(|| anyhow!("Missing required argument: value. Use --password flag if you want to input securely."))?
            .clone()
        };

        let option = GlobalOption::new(&key, &value);
        let mut echo: ManuallyDrop<GlobalOption> = ManuallyDrop::new(GlobalOption::default());
        let _res = set_global_option(&option, Out::from(&mut echo));
        let _echo = ManuallyDrop::into_inner(echo);
        if password {
            print_content(&format!("{key} => ******").into());
        } else {
            print_content(&format!("{key} => {value}").into());
        }
        Ok(())
    }
}

impl CommandExecutor for SetgCommand {
    fn name(&self) -> &'static str {
        "setg"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let key = matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?
            .clone();
        let value = matches.get_one::<String>("value").cloned();
        let password = matches.get_flag("password");
        self.execute(key, value, password)
    }
}
