use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct ProxyCommand;

impl CommandExecutor for ProxyCommand {
    fn name(&self) -> &'static str {
        "proxy"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod off;
pub mod on;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![Arc::new(on::OnCommand), Arc::new(off::OffCommand)]
}

pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            INTERRUPTED.store(false, Ordering::SeqCst);
            return cmd.run(sub_matches);
        }
    }
    eprintln!("No matching command found. Use --help for usage information.");
    Ok(())
}
