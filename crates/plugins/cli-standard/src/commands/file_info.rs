// file_info.rs
// Get remote file infomation
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct FileInfoCommand;

impl FileInfoCommand {
    #[allow(unused_variables)]
    pub fn execute(&self, file: String) -> Result<()> {
        let resp: protos::remote::fs::RemoteFileInfoResponse = crate::connection::rpc(
            "file_info",
            &protos::remote::fs::RemoteFileInfoRequest { file: file.clone() },
        )?;
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        match resp.info {
            Some(i) => {
                print_content(&format!("{}: {}", crate::tr("std.label.path"), i.path).into());
                print_content(
                    &format!(
                        "{}: {} {}",
                        crate::tr("std.label.size"),
                        i.size,
                        crate::tr("std.common.bytes")
                    )
                    .into(),
                );
                print_content(
                    &format!(
                        "{}:  {}",
                        crate::tr("std.label.dir"),
                        if i.is_dir {
                            crate::tr("std.common.yes")
                        } else {
                            crate::tr("std.common.no")
                        }
                    )
                    .into(),
                );
            }
            None => print_content(
                &crate::tr("std.err.fs.file_not_found").replace("{file}", &file).into(),
            ),
        }
        Ok(())
    }
}

impl CommandExecutor for FileInfoCommand {
    fn name(&self) -> &'static str {
        "file_info"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let file = matches
            .get_one::<String>("file")
            .ok_or_else(|| {
                anyhow!("{}", crate::tr("std.err.misc.missing_argument").replace("{arg}", "file"))
            })?
            .clone();
        self.execute(file)
    }
}
