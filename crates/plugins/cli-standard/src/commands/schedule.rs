// schedule.rs
// schedule a task
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ScheduleCommand;

impl ScheduleCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, time: String, cmd: Vec<String>) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if cmd.is_empty() {
            return Err(anyhow!("{}", crate::tr("std.err.task.empty_command")));
        }
        let at = crate::local::parse_time(&time).ok_or_else(|| {
            anyhow!("{}", crate::tr("std.err.misc.invalid_time").replace("{time}", &time))
        })?;
        let cmdline = shell_words::join(cmd.iter().map(String::as_str));
        let tasks = crate::local::read_tasks();
        let id = crate::local::next_task_id(&tasks);
        crate::local::add_task(crate::local::TaskEntry {
            id,
            kind: "cmd".into(),
            session: "0".into(),
            at,
            cmd: Some(cmdline.clone()),
            message: None,
            users: vec![],
        })?;
        print_content(
            &crate::tr("std.prompt.task.scheduled_cmd")
                .replace("{id}", &id.to_string())
                .replace("{at}", &crate::local::format_local(at))
                .replace("{time}", &time)
                .replace("{cmdline}", &cmdline)
                .into(),
        );
        Ok(())
    }
}

impl CommandExecutor for ScheduleCommand {
    fn name(&self) -> &'static str {
        "schedule"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let time = matches
            .get_one::<String>("time")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "time"))
            })?
            .clone();
        let cmd: Vec<String> =
            matches.get_many::<String>("cmd").unwrap_or_default().cloned().collect();
        self.execute(time, cmd)
    }
}
