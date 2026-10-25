// lls.rs
// List files in local current directory (client-side)
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct LlsCommand;

impl LlsCommand {
    #[allow(unused_variables)]
    pub fn execute(&self) -> Result<()> {
        let dir = std::env::current_dir().map_err(|e| anyhow!("cannot get cwd: {e}"))?;
        let mut lines = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| anyhow!("cannot read dir: {e}"))? {
            let entry = entry.map_err(|e| anyhow!("read_dir entry: {e}"))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let ft = entry.file_type().map_err(|e| anyhow!("file type: {e}"))?;
            let kind = if ft.is_dir() { "d" } else if ft.is_symlink() { "l" } else { "-" };
            lines.push(format!("{kind}  {name}"));
        }
        lines.sort();
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&format!("{}:\n{}", dir.display(), lines.join("\n")).into());
        Ok(())
    }
}

impl CommandExecutor for LlsCommand {
    fn name(&self) -> &'static str {
        "lls"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        self.execute()
    }
}
