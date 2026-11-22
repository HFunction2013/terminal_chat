// sleep.rs
// Are you sleepy? Why not sleep for a while?
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

pub struct SleepCommand;

impl SleepCommand {
    /// `time` - Aha... sleepy!, required, value_name: TIME, default: 1000
    pub fn execute(&self, time: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content, is_interrupted);
        let time = if time.parse::<i32>().is_ok() { format!("{time}ms") } else { time };
        let duration = humantime::Duration::from_str(&time)?;
        print_content(&crate::tr("std.prompt.misc.sleep_start").replace("{time}", &time).into());
        let start = std::time::Instant::now();
        while start.elapsed() < *duration {
            if is_interrupted().code == 0 {
                print_content(&crate::tr("std.prompt.misc.sleep_interrupted").into());
                return Ok(());
            }
            thread::sleep(Duration::from_millis(1));
        }
        print_content(&crate::tr("std.prompt.misc.sleep_done").into());

        Ok(())
    }
}

impl CommandExecutor for SleepCommand {
    fn name(&self) -> &'static str {
        "sleep"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let time = matches
            .get_one::<String>("time")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "time"))
            })?
            .clone();
        self.execute(time)
    }
}
