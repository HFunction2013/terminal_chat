// import.rs
// Import workspace information
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ImportCommand;

impl ImportCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, workspace_file: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if !std::path::Path::new(&workspace_file).is_file() {
            print_content(
                &crate::tr("std.err.workspace.file_not_found")
                    .replace("{workspace_file}", &workspace_file)
                    .into(),
            );
            return Ok(());
        }
        let status = std::process::Command::new("tar")
            .args(["-xzf", &workspace_file, "-C"])
            .arg(crate::local::workspaces_dir())
            .status()?;
        if !status.success() {
            print_content(&crate::tr("std.err.workspace.import_failed").into());
            return Ok(());
        }
        print_content(
            &crate::tr("std.prompt.workspace.imported")
                .replace("{workspace_file}", &workspace_file)
                .into(),
        );
        Ok(())
    }
}

impl CommandExecutor for ImportCommand {
    fn name(&self) -> &'static str {
        "import"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let workspace_file = matches
            .get_one::<String>("workspace_file")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "workspace_file")
                )
            })?
            .clone();
        self.execute(workspace_file)
    }
}
