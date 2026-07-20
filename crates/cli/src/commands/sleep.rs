// sleep.rs
// Are you sleepy? Why not sleep for a while?
use clap::ArgMatches;
use anyhow::Result;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use crate::INTERRUPTED;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use cli_core::print_content::print_content;

pub struct SleepCommand;

impl CommandExecutor for SleepCommand {
    fn name(&self) -> &'static str {
        "sleep"
    }

    fn run(&self, _matches: &ArgMatches) -> Result<()> {
        let total = _matches.get_one::<humantime::Duration>("milliseconds").copied().unwrap();
        print_content(format!("[*] Aha... Sleep for {}.", total).as_str());
        let start = std::time::Instant::now();
        while start.elapsed() < *total {
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