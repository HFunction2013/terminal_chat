// delete.rs
// Delete workspace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct DeleteCommand;

impl DeleteCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, workspace_name: Option<String>, force: bool) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let name = workspace_name.unwrap_or_else(crate::local::current_workspace);
        let dir = crate::local::workspace_path(&name);
        if !dir.is_dir() {
            print_content(
                &crate::tr("std.err.workspace.not_found").replace("{name}", &name).into(),
            );
            return Ok(());
        }
        if !force {
            let count = std::fs::read_dir(&dir).map(|rd| rd.count()).unwrap_or(0);
            print_content(
                &crate::tr("std.err.workspace.needs_force")
                    .replace("{name}", &name)
                    .replace("{count}", &count.to_string())
                    .into(),
            );
            return Ok(());
        }
        std::fs::remove_dir_all(&dir)?;
        if crate::local::current_workspace() == name {
            crate::local::set_current_workspace("default")?;
        }
        print_content(&crate::tr("std.prompt.workspace.deleted").replace("{name}", &name).into());
        Ok(())
    }
}

impl CommandExecutor for DeleteCommand {
    fn name(&self) -> &'static str {
        "delete"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let workspace_name = matches.get_one::<String>("workspace_name").cloned();
        let force = matches.get_flag("force");
        self.execute(workspace_name, force)
    }
}
