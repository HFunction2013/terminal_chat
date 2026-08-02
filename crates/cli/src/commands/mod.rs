use crate::INTERRUPTED;
use anyhow::Result;
use clap::ArgMatches;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub trait CommandExecutor {
    fn name(&self) -> &'static str;

    fn run(&self, matches: &ArgMatches) -> Result<()>;
}
pub mod away;
pub mod background;
pub mod ban;
pub mod call;
pub mod cancel;
pub mod cd;
pub mod checksum;
pub mod clear;
pub mod connect;
pub mod create;
pub mod create_freq;
pub mod delete;
pub mod deop;
pub mod disband;
pub mod disconnect;
pub mod download;
pub mod editor;
pub mod file_info;
pub mod foreground;
pub mod freq_info;
pub mod get;
pub mod getg;
pub mod ignore;
pub mod join_freq;
pub mod kick;
pub mod leave_freq;
pub mod load;
#[cfg(debug_assertions)]
pub mod logs;
pub mod ls;
pub mod mkdir;
pub mod msg;
pub mod mute;
pub mod op;
pub mod proxy;
pub mod pwd;
pub mod reconnect;
pub mod reply;
pub mod route;
pub mod run;
pub mod schedule;
pub mod schedule_msg;
pub mod sessions;
pub mod set;
pub mod setg;
pub mod show;
pub mod sleep;
pub mod status;
pub mod tasks;
pub mod tcpm;
pub mod trop;
pub mod unban;
pub mod unignore;
pub mod unload;
pub mod unmute;
pub mod unset;
pub mod unsetg;
pub mod upload;
pub mod user_info;
pub mod whoami;

pub fn all_commands() -> Vec<Arc<dyn CommandExecutor>> {
    vec![
        Arc::new(setg::SetgCommand),
        Arc::new(get::GetCommand),
        Arc::new(getg::GetgCommand),
        Arc::new(set::SetCommand),
        Arc::new(route::RouteCommand),
        Arc::new(proxy::ProxyCommand),
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
        Arc::new(unsetg::UnsetgCommand),
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
        Arc::new(editor::EditorCommand),
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
