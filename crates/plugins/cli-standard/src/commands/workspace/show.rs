// show.rs
// Show information for a certain workspace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ShowCommand;

impl ShowCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, workspace_name: Option<String>) -> Result<()> {
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
        let files = std::fs::read_dir(&dir).map(|rd| rd.count()).unwrap_or(0);
        let size = crate::local::dir_size(&dir);
        print_content(&format!("{}: {}", crate::tr("std.label.workspace"), name).into());
        print_content(&format!("{}:      {}", crate::tr("std.label.path"), dir.display()).into());
        print_content(&format!("{}:   {}", crate::tr("std.label.entries"), files).into());
        print_content(
            &format!(
                "{}:      {} {}",
                crate::tr("std.label.size"),
                size,
                crate::tr("std.common.bytes")
            )
            .into(),
        );
        if name == crate::local::current_workspace() {
            print_content(
                &format!(
                    "{}:    {}",
                    crate::tr("std.label.status"),
                    crate::tr("std.status.current")
                )
                .into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let workspace_name = matches.get_one::<String>("workspace_name").cloned();
        self.execute(workspace_name)
    }
}
