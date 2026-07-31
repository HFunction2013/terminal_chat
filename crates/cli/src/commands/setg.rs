// setg.rs
// Set global options
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use cli_core::global_settings::{GlobalOption, set_global_option};
use cli_core::print_content::print_content;

pub struct SetgCommand;

impl CommandExecutor for SetgCommand {
    fn name(&self) -> &'static str {
        "setg"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        let key = _matches
            .get_one::<String>("key")
            .ok_or_else(|| anyhow!("Missing required argument: key"))?;
        let use_password = _matches.get_flag("password");
        let value = if use_password {
            rpassword::prompt_password(format!("Enter value for '{key}': "))?.trim().to_string()
        } else {
            _matches.get_one::<String>("value")
                    .ok_or_else(|| anyhow!("Missing required argument: value. Use --password flag if you want to input securely."))?
                    .clone()
        };

        let option = GlobalOption::new(key, &value);
        set_global_option(option);
        if use_password {
            print_content(format!("{key} => ******").as_str());
        } else {
            print_content(format!("{key} => {value}").as_str());
        }
        Ok(())
    }
}
