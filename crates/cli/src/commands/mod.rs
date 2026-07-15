use clap::ArgMatches;
use std::sync::Arc;
use anyhow::Result;
use std::sync::atomic::Ordering;
use crate::INTERRUPTED;

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
pub mod sleep;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(setg::SetgCommand),
        Arc::new(set::SetCommand),
        Arc::new(route::RouteCommand),
        Arc::new(proxy_off::ProxyOffCommand),
        Arc::new(show::ShowCommand),
        Arc::new(join_freq::JoinFreqCommand),
        Arc::new(create_freq::CreateFreqCommand),
        Arc::new(background::BackgroundCommand),
        Arc::new(sessions::SessionsCommand),
        Arc::new(load::LoadCommand),
        Arc::new(unload::UnloadCommand),
        Arc::new(clear::ClearCommand),
        Arc::new(tcpm::TcpmCommand),
        Arc::new(upload::UploadCommand),
        Arc::new(download::DownloadCommand),
        Arc::new(checksum::ChecksumCommand),
        Arc::new(cd::CdCommand),
        Arc::new(pwd::PwdCommand),
        Arc::new(ls::LsCommand),
        Arc::new(mkdir::MkdirCommand),
        Arc::new(file_info::FileInfoCommand),
        Arc::new(leave_freq::LeaveFreqCommand),
        Arc::new(call::CallCommand),
        Arc::new(msg::MsgCommand),
        Arc::new(schedule_msg::ScheduleMsgCommand),
        Arc::new(cancel::CancelCommand),
        Arc::new(schedule::ScheduleCommand),
        Arc::new(tasks::TasksCommand),
        Arc::new(whoami::WhoamiCommand),
        Arc::new(unset::UnsetCommand),
        Arc::new(create::CreateCommand),
        Arc::new(run::RunCommand),
        Arc::new(delete::DeleteCommand),
        Arc::new(status::StatusCommand),
        Arc::new(freq_info::FreqInfoCommand),
        Arc::new(reply::ReplyCommand),
        Arc::new(ignore::IgnoreCommand),
        Arc::new(unignore::UnignoreCommand),
        Arc::new(user_info::UserInfoCommand),
        Arc::new(away::AwayCommand),
        Arc::new(foreground::ForegroundCommand),
        Arc::new(disconnect::DisconnectCommand),
        Arc::new(reconnect::ReconnectCommand),
        Arc::new(connect::ConnectCommand),
        #[cfg(debug_assertions)]
        Arc::new(logs::LogsCommand),
        Arc::new(kick::KickCommand),
        Arc::new(ban::BanCommand),
        Arc::new(unban::UnbanCommand),
        Arc::new(mute::MuteCommand),
        Arc::new(unmute::UnmuteCommand),
        Arc::new(op::OpCommand),
        Arc::new(deop::DeopCommand),
        Arc::new(trop::TropCommand),
        Arc::new(disband::DisbandCommand),
        Arc::new(sleep::SleepCommand),
    ]
}

pub fn dispatch(matches: &ArgMatches) -> Result<()> {
    for cmd in all_commands() {
        if let Some(sub_matches) = matches.subcommand_matches(cmd.name()) {
            // 每次执行前清零
            INTERRUPTED.store(false, Ordering::SeqCst);
            return cmd.run(sub_matches);
        }
    }
    eprintln!("No matching command found. Use --help for usage information.");
    Ok(())
}
