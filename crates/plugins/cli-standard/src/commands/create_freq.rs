// create_freq.rs
// Create a new frequency channel
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct CreateFreqCommand;

impl CreateFreqCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, freq: String, p: bool, max_people: Option<String>, no_mod: bool) -> Result<()> {
        let password = if p {
            Some(rpassword::prompt_password("Freq password: ")?)
        } else {
            None
        };
        let max_people = match max_people {
            Some(s) => Some(s.parse::<i32>().map_err(|_| anyhow!("invalid max-people `{s}`"))?),
            None => None,
        };
        let resp: protos::frequency::basic::CreateFreqResponse = crate::connection::rpc(
            "create_freq",
            &protos::frequency::basic::CreateFreqRequest {
                freq: freq.clone(),
                password,
                max_people,
                join_after_create: true,
                no_mod,
            },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        print_content(&resp.message.clone().into());
        Ok(())
    }
}

impl CommandExecutor for CreateFreqCommand {
    fn name(&self) -> &'static str {
        "create_freq"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let freq = matches
            .get_one::<String>("freq")
            .ok_or_else(|| anyhow!("Missing required argument: freq"))?
            .clone();
        let p = matches.get_flag("p");
        let max_people = matches.get_one::<String>("max_people").cloned();
        let no_mod = matches.get_flag("no_mod");
        self.execute(freq, p, max_people, no_mod)
    }
}
