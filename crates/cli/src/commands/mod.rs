use clap::ArgMatches;
use anyhow::Result;

pub trait CommandExecutor {
    fn name(&self) -> &'static str;

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
pub mod schedule;
pub mod tasks;
pub mod whoami;
pub mod unset;
pub mod create;
pub mod run;
pub mod delete;
pub mod status;
pub mod freq_info;
pub mod reply;
pub mod ignore;
pub mod unignore;
pub mod user_info;
pub mod away;
pub mod foreground;
pub mod disconnect;
pub mod reconnect;
pub mod connect;
#[cfg(debug_assertions)]
pub mod logs;
pub mod kick;
pub mod ban;
pub mod unban;
pub mod mute;
pub mod unmute;
pub mod op;
pub mod deop;
pub mod trop;
pub mod disband;

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
        Box::new(schedule::ScheduleCommand),
        Box::new(tasks::TasksCommand),
        Box::new(whoami::WhoamiCommand),
        Box::new(unset::UnsetCommand),
        Box::new(create::CreateCommand),
        Box::new(run::RunCommand),
        Box::new(delete::DeleteCommand),
        Box::new(status::StatusCommand),
        Box::new(freq_info::FreqInfoCommand),
        Box::new(reply::ReplyCommand),
        Box::new(ignore::IgnoreCommand),
        Box::new(unignore::UnignoreCommand),
        Box::new(user_info::UserInfoCommand),
        Box::new(away::AwayCommand),
        Box::new(foreground::ForegroundCommand),
        Box::new(disconnect::DisconnectCommand),
        Box::new(reconnect::ReconnectCommand),
        Box::new(connect::ConnectCommand),
        #[cfg(debug_assertions)]
        Box::new(logs::LogsCommand),
        Box::new(kick::KickCommand),
        Box::new(ban::BanCommand),
        Box::new(unban::UnbanCommand),
        Box::new(mute::MuteCommand),
        Box::new(unmute::UnmuteCommand),
        Box::new(op::OpCommand),
        Box::new(deop::DeopCommand),
        Box::new(trop::TropCommand),
        Box::new(disband::DisbandCommand),
    ]
}
