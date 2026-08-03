// editor.rs
// just opens an editor
#[allow(unused_imports)]
use crate::INTERRUPTED;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use clap::ArgMatches;

pub struct EditorCommand;

impl EditorCommand {
    /// `file` - file to open, optional., value_name: FILE
    /// `editor` - set editor
    #[allow(unused_variables)]
	fn execute(&self, file: Option<String>, editor: String) -> Result<()> {
		// TODO: just opens an editor
		println!("Command `editor` is not yet implemented.");
		Ok(())
	}
}

impl CommandExecutor for EditorCommand {
	fn name(&self) -> &'static str {
		"editor"
	}

	#[allow(unused_variables)]
	fn run(&self, matches: &ArgMatches) -> Result<()> {
        let file = matches
            .get_one::<String>("file")
            .cloned();
        let editor = matches
            .get_one::<String>("editor")
            .ok_or_else(|| anyhow!("Missing required argument: editor"))?
            .clone();
        self.execute(file, editor)
	}
}
