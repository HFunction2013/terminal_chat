// lcd.rs
// Change local directory (client-side)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LcdCommand;

impl LcdCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, dir: Option<String>) -> Result<()> {
        let dir = dir.unwrap_or_else(|| ".".to_string());
        std::env::set_current_dir(&dir).map_err(|e| anyhow!("cannot cd to `{dir}`: {e}"))?;
        let cwd = std::env::current_dir().map_err(|e| anyhow!("cannot get cwd: {e}"))?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&format!("now at {}", cwd.display()).into());
        Ok(())
    }
}

impl CommandExecutor for LcdCommand {
    fn name(&self) -> &'static str {
        "lcd"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let dir = matches.get_one::<String>("dir").cloned();
        self.execute(dir)
    }
}
