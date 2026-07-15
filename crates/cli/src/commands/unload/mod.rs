use clap::ArgMatches;
use std::sync::Arc;
use anyhow::Result;
use crate::commands::CommandExecutor;
use crate::INTERRUPTED;
use std::sync::atomic::Ordering;

pub struct UnloadCommand;

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

pub mod plugin;
pub mod namespace;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(plugin::PluginCommand),
        Arc::new(namespace::NamespaceCommand),

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
