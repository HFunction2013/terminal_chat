use clap::ArgMatches;
use anyhow::Result;

pub trait CommandExecutor {
    /// 对应 YAML 里的 command.name
    fn name(&self) -> &'static str;

    /// 执行命令
    fn run(&self, matches: &ArgMatches) -> Result<()>;
}
pub mod setg;
pub mod set;
pub mod route;
pub mod proxy_off;
pub mod show;
pub mod join_freq;
pub mod create_freq;
pub mod background;
pub mod sessions;
pub mod load;
pub mod unload;
pub mod clear;
pub mod tcpm;
pub mod upload;
pub mod download;
pub mod checksum;
pub mod cd;
pub mod pwd;
pub mod ls;
pub mod mkdir;
pub mod file_info;
pub mod leave_freq;
pub mod call;
pub mod msg;
pub mod schedule_msg;
pub mod cancel;
pub mod whoami;

pub fn all_commands() -> Vec<Box<dyn CommandExecutor>> {
    vec![
        Box::new(setg::SetgCommand),
        Box::new(set::SetCommand),
        Box::new(route::RouteCommand),
        Box::new(proxy_off::ProxyOffCommand),
        Box::new(show::ShowCommand),
        Box::new(join_freq::JoinFreqCommand),
        Box::new(create_freq::CreateFreqCommand),
        Box::new(background::BackgroundCommand),
        Box::new(sessions::SessionsCommand),
        Box::new(load::LoadCommand),
        Box::new(unload::UnloadCommand),
        Box::new(clear::ClearCommand),
        Box::new(tcpm::TcpmCommand),
        Box::new(upload::UploadCommand),
        Box::new(download::DownloadCommand),
        Box::new(checksum::ChecksumCommand),
        Box::new(cd::CdCommand),
        Box::new(pwd::PwdCommand),
        Box::new(ls::LsCommand),
        Box::new(mkdir::MkdirCommand),
        Box::new(file_info::FileInfoCommand),
        Box::new(leave_freq::LeaveFreqCommand),
        Box::new(call::CallCommand),
        Box::new(msg::MsgCommand),
        Box::new(schedule_msg::ScheduleMsgCommand),
        Box::new(cancel::CancelCommand),
        Box::new(whoami::WhoamiCommand),
    ]
}
