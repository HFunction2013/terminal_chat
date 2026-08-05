// setg.rs
// Set global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use crate::global_settings::{GlobalOption, set_global_option};
use crate::print_content::print_content;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct SetgCommand;

impl SetgCommand {
    /// `key` - Config key name, required, value_name: KEY
    /// `value` - Target config value, value_name: VALUE
    /// `password` - use rpassword to read the value, conflicts with: value
    #[allow(unused_variables)]
    fn execute(&self, key: String, value: Option<String>, password: bool) -> Result<()> {
        let value = if password {
            rpassword::prompt_password(format!("Enter value for '{key}': "))?.trim().to_string()
        } else {
            value
            .ok_or_else(|| anyhow!("Missing required argument: value. Use --password flag if you want to input securely."))?
            .clone()
        };

        let option = GlobalOption::new(&key, &value);
        set_global_option(option);
        if password {
            print_content(format!("{key} => ******").as_str());
        } else {
            print_content(format!("{key} => {value}").as_str());
        }
        Ok(())
    }
}

impl CommandExecutor for SetgCommand {
    fn name(&self) -> &'static str {
        "setg"
    }

    #[allow(unused_variables)]
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
