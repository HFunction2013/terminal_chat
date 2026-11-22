// unban.rs
// Unban
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UnbanCommand;

impl UnbanCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, user: String) -> Result<()> {
        let resp: protos::frequency::mod_ops::UnbanResponse = crate::connection::rpc(
            "unban_user",
            &protos::frequency::mod_ops::UnbanRequest { user, freq: String::new() },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for UnbanCommand {
    fn name(&self) -> &'static str {
        "unban"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let user = matches
            .get_one::<String>("user")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "user"))
            })?
            .clone();
        self.execute(user)
    }
}
