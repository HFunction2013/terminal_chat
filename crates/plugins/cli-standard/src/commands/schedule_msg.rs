// schedule_msg.rs
// Schedules message
use crate::LIB;
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;
use safer_ffi::prelude::*;

pub struct ScheduleMsgCommand;

impl ScheduleMsgCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, time: String, message: String, users: Vec<String>) -> Result<()> {
        let lib = LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        if message.trim().is_empty() {
            return Err(anyhow!("{}", crate::tr("std.err.task.empty_message")));
        }
        let at = crate::local::parse_time(&time).ok_or_else(|| {
            anyhow!("{}", crate::tr("std.err.misc.invalid_time").replace("{time}", &time))
        })?;
        let tasks = crate::local::read_tasks();
        let id = crate::local::next_task_id(&tasks);
        crate::local::add_task(crate::local::TaskEntry {
            id,
            kind: "msg".into(),
            session: "0".into(),
            at,
            cmd: None,
            message: Some(message.clone()),
            users: users.clone(),
        })?;
        let who = if users.is_empty() { "everybody".to_string() } else { users.join(", ") };
        print_content(
            &crate::tr("std.prompt.task.scheduled_msg")
                .replace("{id}", &id.to_string())
                .replace("{at}", &crate::local::format_local(at))
                .replace("{time}", &time)
                .replace("{who}", &who)
                .replace("{message}", &message)
                .into(),
        );
        Ok(())
    }
}

impl CommandExecutor for ScheduleMsgCommand {
    fn name(&self) -> &'static str {
        "schedule_msg"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let time = matches
            .get_one::<String>("time")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "time"))
            })?
            .clone();
        let message = matches.get_one::<String>("message").cloned().unwrap_or_default();
        let users =
            matches.get_many::<String>("users").unwrap_or_default().cloned().collect::<Vec<_>>();
        self.execute(time, message, users)
    }
}
