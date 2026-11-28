use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::sync::Arc;
pub struct LoadCommand;

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        crate::commands::emit_deprecated("load", "`plugin load` / `namespace load`");
        dispatch(matches)
    }
}

pub mod namespace;
pub mod plugin;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![Arc::new(plugin::PluginCommand), Arc::new(namespace::NamespaceCommand)]
}

pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            return cmd.run(sub_matches);
        }
    }
    eprintln!("No matching command found. Use --help for usage information.");
    Ok(())
}
