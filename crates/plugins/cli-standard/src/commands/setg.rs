// setg.rs
// Set global options
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core::global_settings::GlobalOption;
use libloading::Symbol;

pub struct SetgCommand;

impl SetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    /// `value` - Target config value, value_name: VALUE
    /// `password` - use rpassword to read the value, conflicts with: value
    pub fn execute(&self, key: String, value: Option<String>, password: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<fn(&safer_ffi::String)>;
        let set_global_option: Symbol<fn(&GlobalOption) -> GlobalOption>;
        unsafe {
            print_content = lib
                .get::<fn(&safer_ffi::String)>(b"print_content")
                .expect("Failed to get `print_content`");
            set_global_option = lib
                .get::<fn(&GlobalOption) -> GlobalOption>(b"set_global_option")
                .expect("Failed to get `set_global_option`");
        }

        let value = if password {
            rpassword::prompt_password(format!("Enter value for '{key}': "))?.trim().to_string()
        } else {
            value
            .ok_or_else(|| anyhow!("Missing required argument: value. Use --password flag if you want to input securely."))?
            .clone()
        };

        let option = GlobalOption::new(&key, &value);
        set_global_option(&option);
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
