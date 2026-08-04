use clap::ArgMatches;
use std::sync::Arc;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use crate::commands::CommandExecutor;
use crate::INTERRUPTED;
use std::sync::atomic::Ordering;

pub struct WorkspaceCommand;

impl CommandExecutor for WorkspaceCommand {
    fn name(&self) -> &'static str {
        "workspace"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod new;
pub mod rename;
pub mod delete;
pub mod switch;
pub mod list;
pub mod show;
pub mod clone;
pub mod export;
pub mod import;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(new::NewCommand),
        Arc::new(rename::RenameCommand),
        Arc::new(delete::DeleteCommand),
        Arc::new(switch::SwitchCommand),
        Arc::new(list::ListCommand),
        Arc::new(show::ShowCommand),
        Arc::new(clone::CloneCommand),
        Arc::new(export::ExportCommand),
        Arc::new(import::ImportCommand),

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
