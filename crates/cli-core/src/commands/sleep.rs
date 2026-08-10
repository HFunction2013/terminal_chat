// sleep.rs
// Are you sleepy? Why not sleep for a while?
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use crate::print_content::print_content;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

pub struct SleepCommand;

impl SleepCommand {
    /// `time` - Aha... sleepy!, required, value_name: TIME, default: 1000
    pub fn execute(&self, time: String) -> Result<()> {
        let time = if time.parse::<i32>().is_ok() { format!("{time}ms") } else { time };
        let duration = humantime::Duration::from_str(&time)?;
        print_content(format!("[*] Aha... Sleep for {time}."));
        let start = std::time::Instant::now();
        while start.elapsed() < *duration {
            if INTERRUPTED.load(Ordering::SeqCst) {
                print_content("[!] All your fault! My dream was disturbed!");
                INTERRUPTED.store(false, Ordering::SeqCst);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(1));
        }
        print_content("[*] Aha... What a nice sleep!");

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
