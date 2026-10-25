// lpwd.rs
// Print local working directory (client-side)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LpwdCommand;

impl LpwdCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let cwd = std::env::current_dir().map_err(|e| anyhow!("cannot get cwd: {e}"))?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&cwd.display().to_string().into());
        Ok(())
    }
}

impl CommandExecutor for LpwdCommand {
    fn name(&self) -> &'static str {
        "lpwd"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
