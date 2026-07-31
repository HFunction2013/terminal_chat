use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use std::sync::Arc;
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

pub mod active_freqs;
pub mod active_users;
pub mod globals;
pub mod locals;
pub mod moderators;
pub mod options;
pub mod variables;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(options::OptionsCommand),
        Arc::new(globals::GlobalsCommand),
        Arc::new(locals::LocalsCommand),
        Arc::new(variables::VariablesCommand),
        Arc::new(active_freqs::ActiveFreqsCommand),
        Arc::new(active_users::ActiveUsersCommand),
        Arc::new(moderators::ModeratorsCommand),
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
