// update.rs
// Update plugins.
// 'standard' means main program.
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct UpdateCommand;

impl UpdateCommand {
    /// `plugin` - Plugin(s) to update, required, value_name: PLUGIN_NAME
    /// `registry` - Set plugin registry
    /// `reload` - Reload after update
    /// `force` - Override old one (if exists).
    #[allow(unused_variables)]
	fn execute(&self, plugin: String, registry: Option<String>, reload: bool, force: bool) -> Result<()> {
		// TODO: Update plugins.
		// TODO: 'standard' means main program.
		println!("Command `update` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for UpdateCommand {
	fn name(&self) -> &'static str {
		"update"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| anyhow!("Missing required argument: plugin"))?
            .clone();
        let registry = matches
            .get_one::<String>("registry")
            .cloned();
        let reload = matches.get_flag("reload");
        let force = matches.get_flag("force");
        self.execute(plugin, registry, reload, force)
	}
}
