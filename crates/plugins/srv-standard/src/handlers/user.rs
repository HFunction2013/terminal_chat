//! //! 用户：user_info / whoami / ignore / unignore / set_presence

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use std::ops::Not;
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_user_info(sender: String, req: protos::messaging::user::UserInfoRequest) -> protos::messaging::user::UserInfoResponse {
    use protos::messaging::user::{UserInfo, UserInfoResponse};
    let state = STATE.lock().unwrap();
    match state.users.get(&req.user) {
        Some(u) => {
            let info = UserInfo {
                user: u.user.clone(),
                nickname: u.nickname.clone(),
                ip_address: u.user.rsplit('@').next().unwrap_or("").to_string(),
                online: u.online,
                freq: u.freq.clone(),
                connected_at: u.connected_at as i64,
                is_moderator: u
                    .freq
                    .as_str()
                    .is_empty()
                    .not()
                    .then(|| state.freqs.get(&u.freq).map(|f| f.members.get(&u.user).map(|m| m.is_moderator).unwrap_or(false)).unwrap_or(false))
                    .unwrap_or(false),
                is_creator: u.freq.as_str().is_empty().not()
                    .then(|| state.freqs.get(&u.freq).map(|f| f.creator == u.user).unwrap_or(false))
                    .unwrap_or(false),
                ignored_you: u.ignored.contains(&sender),
            };
            UserInfoResponse {
                found: true,
                info: Some(info),
                message: "found".into(),
            }
        }
        None => UserInfoResponse { found: false, info: None, message: "user not found".into() },
    }
}

pub(crate) fn handle_whoami(sender: String, _req: protos::messaging::user::WhoamiRequest) -> protos::messaging::user::WhoamiResponse {
    use protos::messaging::user::WhoamiResponse;
    WhoamiResponse {
        user: user_part(&sender),
        full_user: sender.clone(),
        success: true,
        message: format!("you are {}", sender),
    }
}

pub(crate) fn handle_ignore(sender: String, req: protos::messaging::user::IgnoreRequest) -> protos::messaging::user::IgnoreResponse {
    use protos::messaging::user::IgnoreResponse;
    let mut state = STATE.lock().unwrap();
    if !state.users.contains_key(&req.user) {
        return IgnoreResponse { success: false, message: format!("user `{}` not found", req.user) };
    }
    ensure_user(&mut state, &sender);
    state.users.get_mut(&sender).unwrap().ignored.insert(req.user.clone());
    IgnoreResponse { success: true, message: format!("you are now ignoring {}", req.user) }
}

pub(crate) fn handle_unignore(sender: String, req: protos::messaging::user::UnignoreRequest) -> protos::messaging::user::UnignoreResponse {
    use protos::messaging::user::UnignoreResponse;
    let mut state = STATE.lock().unwrap();
    let removed = state
        .users
        .get_mut(&sender)
        .map(|u| u.ignored.remove(&req.user))
        .unwrap_or(false);
    UnignoreResponse {
        success: removed,
        message: if removed {
            format!("you are no longer ignoring {}", req.user)
        } else {
            format!("you were not ignoring {}", req.user)
        },
    }
}

pub(crate) fn handle_set_presence(sender: String, req: protos::connection::session::SetPresenceRequest) -> protos::connection::session::SetPresenceResponse {
    use protos::connection::session::{PresenceStatus, SetPresenceResponse};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    let away = match req.status() {
        PresenceStatus::PresenceAway => "away",
        PresenceStatus::PresenceBusy => "busy",
        _ => "",
    };
    state.users.get_mut(&sender).unwrap().away = away.to_string();
    if let Some(freq) = user_freq(&state, &sender)
        && let Some(m) = state.freqs.get_mut(&freq).and_then(|f| f.members.get_mut(&sender))
    {
        m.away = away.to_string();
    }
    SetPresenceResponse {
        success: true,
        status: req.status,
        message: format!("presence set to `{}`", if away.is_empty() { "online" } else { away }),
    }
}
