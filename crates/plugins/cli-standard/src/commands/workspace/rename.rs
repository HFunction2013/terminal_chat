// rename.rs
// Rename a workspace
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct RenameCommand;

impl RenameCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, source: Option<String>, target: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let src = source.unwrap_or_else(crate::local::current_workspace);
        let s = crate::local::workspace_path(&src);
        let t = crate::local::workspace_path(&target);
        if !s.is_dir() {
            print_content(&crate::tr("std.err.workspace.not_found").replace("{name}", &src).into());
            return Ok(());
        }
        if t.exists() {
            print_content(
                &crate::tr("std.err.workspace.already_exists").replace("{target}", &target).into(),
            );
            return Ok(());
        }
        std::fs::rename(&s, &t)?;
        if crate::local::current_workspace() == src {
            crate::local::set_current_workspace(&target)?;
        }
        print_content(
            &crate::tr("std.prompt.workspace.renamed")
                .replace("{src}", &src)
                .replace("{target}", &target)
                .into(),
        );
        Ok(())
    }
}

impl CommandExecutor for RenameCommand {
    fn name(&self) -> &'static str {
        "rename"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let source = matches.get_one::<String>("source").cloned();
        let target = matches
            .get_one::<String>("target")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "target"))
            })?
            .clone();
        self.execute(source, target)
    }
}
