// setg.rs
// Set global options
use crate::commands::CommandExecutor;
use crate::global_settings::{GlobalOption, set_global_option};
use crate::print_content::print_content;
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct SetgCommand;

impl SetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    /// `value` - Target config value, value_name: VALUE
    /// `password` - use rpassword to read the value, conflicts with: value
    pub fn execute(&self, key: String, value: Option<String>, password: bool) -> Result<()> {
        let value = if password {
            rpassword::prompt_password(format!("Enter value for '{key}': "))?.trim().to_string()
        } else {
            value
            .ok_or_else(|| anyhow!("Missing required argument: value. Use --password flag if you want to input securely."))?
            .clone()
        };

        let option = GlobalOption::new(&key, &value);
        unsafe {
            set_global_option(option);
        }
        unsafe {
            if password {
                print_content(format!("{key} => ******").into());
            } else {
                print_content(format!("{key} => {value}").into());
            }
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
