use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
use anyhow::Result;
use clap::ArgMatches;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct TcpmCommand;

impl CommandExecutor for TcpmCommand {
    fn name(&self) -> &'static str {
        "tcpm"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod download;
pub mod install;
pub mod list;
pub mod search;
pub mod show;
pub mod uninstall;
pub mod update;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(update::UpdateCommand),
        Arc::new(search::SearchCommand),
        Arc::new(show::ShowCommand),
        Arc::new(list::ListCommand),
        Arc::new(install::InstallCommand),
        Arc::new(uninstall::UninstallCommand),
        Arc::new(download::DownloadCommand),
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
