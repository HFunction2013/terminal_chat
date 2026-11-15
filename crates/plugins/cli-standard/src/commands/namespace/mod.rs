//! `namespace` 一级命令：命名空间维度的 `load` / `unload`。
//!
//! 复用 `load namespace` / `unload namespace` 既有执行逻辑；旧顶层
//! `load` / `unload` 命令保留为兼容别名，不在此模块移除。
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::sync::Arc;

pub struct NamespaceCommand;

impl CommandExecutor for NamespaceCommand {
    fn name(&self) -> &'static str {
        "namespace"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

struct LoadCommand;

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let namespace = matches
            .get_one::<String>("namespace")
            .cloned()
            .ok_or_else(|| anyhow!("Missing required argument: namespace"))?;
        crate::commands::load::namespace::NamespaceCommand.execute(namespace)
    }
}

struct UnloadCommand;

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let namespace_name = matches
            .get_one::<String>("namespace_name")
            .cloned()
            .ok_or_else(|| anyhow!("Missing required argument: namespace_name"))?;
        crate::commands::unload::namespace::NamespaceCommand.execute(namespace_name)
    }
}

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![Arc::new(LoadCommand), Arc::new(UnloadCommand)]
}

pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            return cmd.run(sub_matches);
        }
    }
    Ok(())
}
