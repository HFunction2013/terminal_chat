// install.rs
// Install plugin
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct InstallCommand;

impl InstallCommand {
    #[allow(unused_variables)]
    pub fn execute(
        &self,
        plugin: String,
        registry: Option<String>,
        uload: bool,
        local: Option<String>,
        force: bool,
    ) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let _ = (registry, force);
        let local_path = match local {
            Some(p) => p,
            None => {
                print_content(&crate::tr("std.err.plugin.local_install_only").into());
                return Ok(());
            }
        };
        if !std::path::Path::new(&local_path).is_file() {
            print_content(
                &crate::tr("std.err.plugin.local_not_found")
                    .replace("{local_path}", &local_path)
                    .into(),
            );
            return Ok(());
        }
        let file_name = std::path::Path::new(&local_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| plugin.clone());
        let dest = crate::local::plugins_dir().join(&file_name);
        std::fs::copy(&local_path, &dest)?;
        let mut installed = crate::local::read_installed();
        if !installed.contains(&file_name) {
            installed.push(file_name.clone());
            crate::local::write_installed(&installed)?;
        }
        print_content(
            &crate::tr("std.prompt.plugin.installed")
                .replace("{file_name}", &file_name)
                .replace("{dest}", &dest.display().to_string())
                .into(),
        );
        if uload {
            cli_core_macros::load_core_symbols!(lib, load_plugin);
            let res = load_plugin(&file_name.into());
            print_content(
                &format!(
                    "[{}] {}",
                    if res.success == 1 { "OK" } else { "ERR" },
                    res.msg.into_rust().map(|s| s.to_string()).unwrap_or_default()
                )
                .into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for InstallCommand {
    fn name(&self) -> &'static str {
        "install"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let plugin = matches
            .get_one::<String>("plugin")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "plugin"))
            })?
            .clone();
        let registry = matches.get_one::<String>("registry").cloned();
        let uload = matches.get_flag("uload");
        let local = matches.get_one::<String>("local").cloned();
        let force = matches.get_flag("force");
        self.execute(plugin, registry, uload, local, force)
    }
}
