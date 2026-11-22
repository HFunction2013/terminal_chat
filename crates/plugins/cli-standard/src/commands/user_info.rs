// user_info.rs
// Show user information
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UserInfoCommand;

impl UserInfoCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, user: String) -> Result<()> {
        let resp: protos::messaging::user::UserInfoResponse = crate::connection::rpc(
            "user_info",
            &protos::messaging::user::UserInfoRequest { user: user.clone() },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        match resp.info {
            Some(info) => {
                print_content(&format!("{}:   {}", crate::tr("std.label.user"), info.user).into());
                print_content(
                    &format!("{}:   {}", crate::tr("std.label.name"), info.nickname).into(),
                );
                print_content(
                    &format!("{}:     {}", crate::tr("std.label.ip"), info.ip_address).into(),
                );
                print_content(
                    &format!(
                        "{}: {}",
                        crate::tr("std.label.online"),
                        if info.online {
                            crate::tr("std.common.yes")
                        } else {
                            crate::tr("std.common.no")
                        }
                    )
                    .into(),
                );
                if !info.freq.is_empty() {
                    print_content(
                        &format!("{}:   {}", crate::tr("std.label.freq"), info.freq).into(),
                    );
                }
                print_content(
                    &format!(
                        "{}:    {}",
                        crate::tr("std.label.mod"),
                        if info.is_moderator {
                            crate::tr("std.common.yes")
                        } else {
                            crate::tr("std.common.no")
                        }
                    )
                    .into(),
                );
                print_content(
                    &format!(
                        "{}:  {}",
                        crate::tr("std.label.owner"),
                        if info.is_creator {
                            crate::tr("std.common.yes")
                        } else {
                            crate::tr("std.common.no")
                        }
                    )
                    .into(),
                );
                if info.ignored_you {
                    print_content(&crate::tr("std.prompt.user.ignores_you").into());
                }
            }
            None => {
                print_content(&crate::tr("std.err.user.not_found").replace("{user}", &user).into())
            }
        }
        Ok(())
    }
}

impl CommandExecutor for UserInfoCommand {
    fn name(&self) -> &'static str {
        "user_info"
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
