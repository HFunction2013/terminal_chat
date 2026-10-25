//! //! 频点：create / join / leave / info / disband / active 系列

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_create_freq(sender: String, req: protos::frequency::basic::CreateFreqRequest) -> protos::frequency::basic::CreateFreqResponse {
    use protos::frequency::basic::{CreateFreqResponse, CreateResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateInvalidParam as i32,
            message: format!("`{}` is not a valid freq: must be a decimal in [0, 1]", req.freq),
        };
    }
    if state.freqs.contains_key(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateDuplicate as i32,
            message: format!("freq `{}` already exists", req.freq),
        };
    }
    let now = now_ts();
    state.freqs.insert(
        req.freq.clone(),
        FreqRoom {
            name: req.freq.clone(),
            password: req.password.clone(),
            max_members: req.max_people.unwrap_or(0),
            creator: sender.clone(),
            members: HashMap::new(),
            banned: HashSet::new(),
            no_mod: req.no_mod,
            #[allow(dead_code)]
            created_at: now,
        },
    );
    state.users.get_mut(&sender).unwrap().freq = req.freq.clone();
    // 创建者自动成为第一个成员（且是 moderator 语义上的 creator）。
    state.freqs.get_mut(&req.freq).unwrap().members.insert(
        sender.clone(),
        Member {
            user: sender.clone(),
            nickname: user_part(&sender),
            is_moderator: true,
            away: String::new(),
            muted_until: 0,
            joined_at: now,
        },
    );
    println!("[srv-standard] freq `{}` created by {} ({} joined)", req.freq, user_part(&sender), user_part(&sender));
    CreateFreqResponse {
        result: R::CreateSuccess as i32,
        message: format!("freq `{}` created, joined as creator", req.freq),
    }
}

pub(crate) fn handle_join_freq(sender: String, req: protos::frequency::basic::JoinFreqRequest) -> protos::frequency::basic::JoinFreqResponse {
    use protos::frequency::basic::{JoinFreqResponse, FreqStatus as S};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return JoinFreqResponse {
            status: S::FreqFormatWrong as i32,
            message: format!("`{}` is not a valid freq: must be a decimal in [0, 1]", req.freq),
        };
    }
    let room = match state.freqs.get_mut(&req.freq) {
        Some(r) => r,
        None => {
            return JoinFreqResponse {
                status: S::FreqError as i32,
                message: format!("freq `{}` does not exist; use `create_freq` first", req.freq),
            }
        }
    };
    if room.banned.contains(&sender) {
        return JoinFreqResponse {
            status: S::FreqError as i32,
            message: "you are banned from this freq".into(),
        };
    }
    if let Some(pw) = &room.password {
        if req.password.as_deref() != Some(pw.as_str()) {
            return JoinFreqResponse {
                status: S::FreqNeedPassword as i32,
                message: "wrong password for this freq".into(),
            };
        }
    }
    if room.max_members > 0 && room.members.len() as i32 >= room.max_members {
        return JoinFreqResponse {
            status: S::FreqFull as i32,
            message: format!("freq `{}` is full ({})", req.freq, room.max_members),
        };
    }
    if room.members.contains_key(&sender) {
        return JoinFreqResponse {
            status: S::FreqIdle as i32,
            message: format!("already in freq `{}`", req.freq),
        };
    }
    let now = now_ts();
    let count;
    {
        let room = state.freqs.get_mut(&req.freq).expect("checked above");
        room.members.insert(
            sender.clone(),
            Member {
                user: sender.clone(),
                nickname: user_part(&sender),
                is_moderator: false,
                away: String::new(),
                muted_until: 0,
                joined_at: now,
            },
        );
        count = room.members.len();
    }
    state.users.get_mut(&sender).unwrap().freq = req.freq.clone();
    println!(
        "[srv-standard] {} joined freq `{}` (members={})",
        user_part(&sender),
        req.freq,
        count
    );
    JoinFreqResponse {
        status: S::FreqIdle as i32,
        message: format!("joined freq `{}` (members={count})", req.freq),
    }
}

pub(crate) fn handle_leave_freq(sender: String, _req: protos::utils::empty::Empty) -> protos::frequency::basic::LeaveFreqResponse {
    use protos::frequency::basic::{LeaveFreqResponse, LeaveResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return LeaveFreqResponse {
                result: R::LeaveNotJoined as i32,
                message: "you are not in any freq".into(),
            }
        }
    };
    // 创建者不能直接离开（除非 -n/--no-mod 无版主频点）。
    let no_mod = state.freqs.get(&freq).map(|f| f.no_mod).unwrap_or(false);
    if state.freqs.get(&freq).map(|f| f.creator == sender).unwrap_or(false) && !no_mod {
        return LeaveFreqResponse {
            result: R::LeaveFailed as i32,
            message: "you are the creator; use `disband` to remove the freq".into(),
        };
    }
    if let Some(room) = state.freqs.get_mut(&freq) {
        room.members.remove(&sender);
    }
    state.users.get_mut(&sender).unwrap().freq = String::new();
    // 无版主频点的创建者离开后，若不再有人则自动解散。
    if no_mod && state.freqs.get(&freq).map(|f| f.members.is_empty()).unwrap_or(false) {
        state.freqs.remove(&freq);
        VFS.lock().unwrap().remove(&freq);
        println!("[srv-standard] freq `{freq}` auto-disbanded (no_mod creator left)");
        return LeaveFreqResponse {
            result: R::LeaveSuccess as i32,
            message: format!("left freq `{freq}` (auto-disbanded)"),
        };
    }
    println!("[srv-standard] {} left freq `{}`", user_part(&sender), freq);
    LeaveFreqResponse {
        result: R::LeaveSuccess as i32,
        message: format!("left freq `{freq}`"),
    }
}

pub(crate) fn handle_freq_info(_sender: String, req: protos::frequency::basic::FreqInfoRequest) -> protos::frequency::basic::FreqInfoResponse {
    use protos::frequency::basic::{FreqInfo, FreqInfoResponse, MemberInfo};
    let state = STATE.lock().unwrap();
    match state.freqs.get(&req.freq) {
        Some(room) => {
            let members = room
                .members
                .values()
                .map(|m| MemberInfo {
                    nickname: m.nickname.clone(),
                    ip_address: m.user.rsplit('@').next().unwrap_or("").to_string(),
                })
                .collect();
            let info = FreqInfo {
                freq: room.name.clone(),
                has_password: room.password.is_some(),
                current_members: room.members.len() as i32,
                max_members: room.max_members,
                creator_nickname: user_part(&room.creator),
                creator_ip: room.creator.rsplit('@').next().unwrap_or("").to_string(),
                members,
            };
            FreqInfoResponse { found: true, info: Some(info), message: "found".into() }
        }
        None => FreqInfoResponse { found: false, info: None, message: format!("freq `{}` not found", req.freq) },
    }
}

pub(crate) fn handle_disband_freq(sender: String, req: protos::frequency::mod_ops::DisbandRequest) -> protos::frequency::mod_ops::DisbandResponse {
    use protos::frequency::mod_ops::{DisbandResponse, DisbandResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return DisbandResponse {
                    result: R::DisbandNotFound as i32,
                    message: "you are not in any freq".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return DisbandResponse {
                result: R::DisbandNotFound as i32,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    if room.creator != sender {
        return DisbandResponse {
            result: R::DisbandAccessDenied as i32,
            message: "only the creator can disband the freq".into(),
        };
    }
    if room.members.len() > 1 {
        return DisbandResponse {
            result: R::DisbandHasMembers as i32,
            message: format!("freq `{freq}` still has {} members", room.members.len()),
        };
    }
    state.freqs.remove(&freq);
    state.users.get_mut(&sender).unwrap().freq = String::new();
    // 频点销毁时同时销毁其共享 VFS（freq 内共享，无私有文件系统）。
    VFS.lock().unwrap().remove(&freq);
    println!("[srv-standard] freq `{freq}` disbanded by {}", user_part(&sender));
    DisbandResponse {
        result: R::DisbandSuccess as i32,
        message: format!("freq `{freq}` disbanded"),
    }
}

pub(crate) fn handle_active_freqs(_sender: String, _req: protos::frequency::status::ActiveFreqsRequest) -> protos::frequency::status::ActiveFreqsResponse {
    use protos::frequency::status::{ActiveFreq, ActiveFreqsResponse};
    let state = STATE.lock().unwrap();
    let freqs = state
        .freqs
        .values()
        .map(|r| ActiveFreq {
            freq: r.name.clone(),
            current_members: r.members.len() as i32,
            max_members: r.max_members,
            has_password: r.password.is_some(),
            creator: user_part(&r.creator),
        })
        .collect();
    ActiveFreqsResponse {
        freqs,
        success: true,
        message: format!("{} active freq(s)", state.freqs.len()),
    }
}

pub(crate) fn handle_active_users(sender: String, req: protos::frequency::status::ActiveUsersRequest) -> protos::frequency::status::ActiveUsersResponse {
    use protos::frequency::status::{ActiveUser, ActiveUsersResponse};
    let state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return ActiveUsersResponse {
                    freq: String::new(),
                    users: vec![],
                    success: false,
                    message: "you are not in any freq; specify one".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return ActiveUsersResponse {
                freq: freq.clone(),
                users: vec![],
                success: false,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    let users = room
        .members
        .values()
        .map(|m| ActiveUser {
            user: m.user.clone(),
            nickname: m.nickname.clone(),
            is_moderator: m.is_moderator,
            is_creator: room.creator == m.user,
            away: !m.away.is_empty(),
        })
        .collect();
    ActiveUsersResponse {
        freq: freq.clone(),
        users,
        success: true,
        message: format!("{} user(s) in freq `{freq}`", room.members.len()),
    }
}

pub(crate) fn handle_moderators(sender: String, req: protos::frequency::status::ModeratorsRequest) -> protos::frequency::status::ModeratorsResponse {
    use protos::frequency::status::{ActiveUser, ModeratorsResponse};
    let state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return ModeratorsResponse {
                    freq: String::new(),
                    moderators: vec![],
                    success: false,
                    message: "you are not in any freq; specify one".into(),
                }
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get(&freq) {
        Some(r) => r,
        None => {
            return ModeratorsResponse {
                freq: freq.clone(),
                moderators: vec![],
                success: false,
                message: format!("freq `{freq}` not found"),
            }
        }
    };
    let mods = room
        .members
        .values()
        .filter(|m| m.is_moderator || room.creator == m.user)
        .map(|m| ActiveUser {
            user: m.user.clone(),
            nickname: m.nickname.clone(),
            is_moderator: m.is_moderator,
            is_creator: room.creator == m.user,
            away: !m.away.is_empty(),
        })
        .collect::<Vec<_>>();
    let n = mods.len();
    ModeratorsResponse {
        freq: freq.clone(),
        moderators: mods,
        success: true,
        message: format!("{n} moderator(s) in freq `{freq}`"),
    }
}
