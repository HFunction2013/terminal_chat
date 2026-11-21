// hello.rs
// Greet someone from the example plugin
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct HelloCommand;

impl HelloCommand {
    /// `who` - Who to greet (defaults to world), value_name: WHO
    #[allow(unused_variables)]
    pub fn execute(&self, who: Option<String>) -> Result<()> {
        let who = who.unwrap_or_else(|| "world".to_string());
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&format!("Hello, {who}! (from the example plugin)").into());
        Ok(())
    }
}

impl CommandExecutor for HelloCommand {
    fn name(&self) -> &'static str {
        "hello"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let who = matches.get_one::<String>("who").cloned();
        self.execute(who)
    }
}
