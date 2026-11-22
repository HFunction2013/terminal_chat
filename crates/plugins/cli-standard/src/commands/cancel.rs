// cancel.rs
// cancel a task
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct CancelCommand;

impl CancelCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, task_id: String) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let id: u64 = task_id.parse().map_err(|_| {
            anyhow!("{}", crate::tr("std.err.task.invalid_id").replace("{task_id}", &task_id))
        })?;
        if crate::local::remove_task(id) {
            print_content(
                &crate::tr("std.prompt.task.cancelled").replace("{id}", &id.to_string()).into(),
            );
        } else {
            print_content(
                &crate::tr("std.err.task.not_found").replace("{id}", &id.to_string()).into(),
            );
        }
        Ok(())
    }
}

impl CommandExecutor for CancelCommand {
    fn name(&self) -> &'static str {
        "cancel"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let task_id = matches
            .get_one::<String>("task_id")
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    crate::tr("std.err.misc.missing_argument").replace("{arg}", "task_id")
                )
            })?
            .clone();
        self.execute(task_id)
    }
}
