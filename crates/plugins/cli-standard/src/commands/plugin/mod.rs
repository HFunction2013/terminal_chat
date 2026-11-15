//! `plugin` 一级命令：插件维度的 `list` / `load` / `unload`。
//!
//! 复用 `load` / `unload` / `show plugins` 既有的执行逻辑；旧顶层
//! `load` / `unload` 命令保留为兼容别名，不在此模块移除。
use crate::commands::CommandExecutor;
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use std::sync::Arc;

pub struct PluginCommand;

impl CommandExecutor for PluginCommand {
    fn name(&self) -> &'static str {
        "plugin"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        dispatch(matches)
    }
}

struct ListCommand;

impl CommandExecutor for ListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn run(&self, _: &ArgMatches) -> Result<()> {
        crate::commands::show::plugins::PluginsCommand.execute()
    }
}

struct LoadCommand;

impl CommandExecutor for LoadCommand {
    fn name(&self) -> &'static str {
        "load"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .cloned()
            .ok_or_else(|| anyhow!("Missing required argument: plugin"))?;
        crate::commands::load::plugin::PluginCommand.execute(false, plugin)
    }
}

struct UnloadCommand;

impl CommandExecutor for UnloadCommand {
    fn name(&self) -> &'static str {
        "unload"
    }

    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin_name = matches
            .get_one::<String>("plugin_name")
            .cloned()
            .ok_or_else(|| anyhow!("Missing required argument: plugin_name"))?;
        crate::commands::unload::plugin::PluginCommand.execute(plugin_name)
    }
}

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![Arc::new(ListCommand), Arc::new(LoadCommand), Arc::new(UnloadCommand)]
}

pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            return cmd.run(sub_matches);
        }
    }
    Ok(())
}
