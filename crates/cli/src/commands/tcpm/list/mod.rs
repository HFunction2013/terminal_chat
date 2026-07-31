use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct ListCommand;

impl CommandExecutor for ListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod downloaded;
pub mod installed;
pub mod loaded;
pub mod registry;
pub mod updates;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(installed::InstalledCommand),
        Arc::new(downloaded::DownloadedCommand),
        Arc::new(loaded::LoadedCommand),
        Arc::new(registry::RegistryCommand),
        Arc::new(updates::UpdatesCommand),
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
