// sleep.rs
// Are you sleepy? Why not sleep for a while?
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::sync::atomic::Ordering;
use crate::print_content::print_content;
use std::time::Duration;
use std::thread;

pub struct SleepCommand;

impl SleepCommand {
    /// `milliseconds` - Aha... sleepy!, required, value_name: TIME, default: 1000
    #[allow(unused_variables)]
    fn execute(&self, milliseconds: humantime::Duration) -> Result<()> {
        print_content(format!("[*] Aha... Sleep for {milliseconds}.").as_str());
        let start = std::time::Instant::now();
        while start.elapsed() < *milliseconds {
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

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let milliseconds = matches
            .get_one::<humantime::Duration>("milliseconds")
            .ok_or_else(|| anyhow!("Missing required argument: milliseconds"))?
            .clone();
        self.execute(milliseconds)
    }
}
