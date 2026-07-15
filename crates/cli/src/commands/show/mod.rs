use clap::ArgMatches;
use std::sync::Arc;
use anyhow::Result;
use crate::commands::CommandExecutor;
use crate::INTERRUPTED;
use std::sync::atomic::Ordering;

pub struct ShowCommand;

impl CommandExecutor for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod options;
pub mod afreqs;
pub mod ausers;
pub mod mods;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(options::OptionsCommand),
        Arc::new(afreqs::AfreqsCommand),
        Arc::new(ausers::AusersCommand),
        Arc::new(mods::ModsCommand),

    ]
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
