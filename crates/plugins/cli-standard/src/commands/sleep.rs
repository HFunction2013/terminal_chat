// sleep.rs
// Are you sleepy? Why not sleep for a while?
use crate::LIB;
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use libloading::Symbol;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

pub struct SleepCommand;

impl SleepCommand {
    /// `time` - Aha... sleepy!, required, value_name: TIME, default: 1000
    pub fn execute(&self, time: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        let print_content: Symbol<fn(&safer_ffi::String)>;
        let is_interrupted: Symbol<fn() -> bool>;
        unsafe {
            print_content = lib
                .get::<fn(&safer_ffi::String)>(b"print_content")
                .expect("Failed to get `print_content`");
            is_interrupted =
                lib.get::<fn() -> bool>(b"is_interrupted").expect("Failed to get `is_interrupted`");
        }
        let time = if time.parse::<i32>().is_ok() { format!("{time}ms") } else { time };
        let duration = humantime::Duration::from_str(&time)?;
        print_content(&format!("[*] Aha... Sleep for {time}.").into());
        let start = std::time::Instant::now();
        while start.elapsed() < *duration {
            if is_interrupted() {
                print_content(&"[!] All your fault! My dream was disturbed!".into());
                return Ok(());
            }
            thread::sleep(Duration::from_millis(1));
        }
        print_content(&"[*] Aha... What a nice sleep!".into());

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
            .ok_or_else(|| anyhow!("Missing required argument: time"))?
            .clone();
        self.execute(time)
    }
}
