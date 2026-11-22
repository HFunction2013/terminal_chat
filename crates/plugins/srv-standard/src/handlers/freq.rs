//! //! 频点：create / join / leave / info / disband / active 系列

#[allow(unused_imports)]
use crate::{
    FreqRoom, Member, NOTIFICATIONS, STATE, ServerState, UserProfile, VFS, VfsEntry, VfsUser,
    display_user, ensure_user, mod_auth, norm_path, notify, now_ts, user_freq, user_part,
    valid_freq, vfs_ensure_dirs,
};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

pub(crate) fn handle_create_freq(
    sender: String,
    req: protos::frequency::basic::CreateFreqRequest,
) -> protos::frequency::basic::CreateFreqResponse {
    use protos::frequency::basic::{CreateFreqResponse, CreateResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateInvalidParam as i32,
            message: crate::i18n::t("srv.freq.invalid").replace("{freq}", &req.freq).to_string(),
        };
    }
    if state.freqs.contains_key(&req.freq) {
        return CreateFreqResponse {
            result: R::CreateDuplicate as i32,
            message: crate::i18n::t("srv.freq.exists").replace("{freq}", &req.freq).to_string(),
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
    println!(
        "[srv-standard] freq `{}` created by {} ({} joined)",
        req.freq,
        user_part(&sender),
        user_part(&sender)
    );
    CreateFreqResponse {
        result: R::CreateSuccess as i32,
        message: crate::i18n::t("srv.freq.created").replace("{freq}", &req.freq).to_string(),
    }
}

pub(crate) fn handle_join_freq(
    sender: String,
    req: protos::frequency::basic::JoinFreqRequest,
) -> protos::frequency::basic::JoinFreqResponse {
    use protos::frequency::basic::{FreqStatus as S, JoinFreqResponse};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    if !valid_freq(&req.freq) {
        return JoinFreqResponse {
            status: S::FreqFormatWrong as i32,
            message: crate::i18n::t("srv.freq.invalid").replace("{freq}", &req.freq).to_string(),
        };
    }
    let room = match state.freqs.get_mut(&req.freq) {
        Some(r) => r,
        None => {
            return JoinFreqResponse {
                status: S::FreqError as i32,
                message: crate::i18n::t("srv.freq.join_missing")
                    .replace("{freq}", &req.freq)
                    .to_string(),
            };
        }
    };
    if room.banned.contains(&sender) {
        return JoinFreqResponse {
            status: S::FreqError as i32,
            message: crate::i18n::t("srv.freq.banned").to_string(),
        };
    }
    if let Some(pw) = &room.password {
        if req.password.as_deref() != Some(pw.as_str()) {
            return JoinFreqResponse {
                status: S::FreqNeedPassword as i32,
                message: crate::i18n::t("srv.freq.wrong_password").to_string(),
            };
        }
    }
    if room.max_members > 0 && room.members.len() as i32 >= room.max_members {
        return JoinFreqResponse {
            status: S::FreqFull as i32,
            message: crate::i18n::t("srv.freq.full")
                .replace("{freq}", &req.freq)
                .replace("{count}", &room.max_members.to_string())
                .to_string(),
        };
    }
    if room.members.contains_key(&sender) {
        // 已在目标频点：校正当前频点上下文（可能因之前的挂起/切换残留漂移）。
        state.users.get_mut(&sender).unwrap().freq = req.freq.clone();
        return JoinFreqResponse {
            status: S::FreqIdle as i32,
            message: crate::i18n::t("srv.freq.already_in").replace("{freq}", &req.freq).to_string(),
        };
    }
    let now = now_ts();
    let count;
    {
        // 一个用户同时只属于一个频点：加入新频点前，先从旧频点成员列表移除，
        // 避免旧频点成员残留导致 active_users 仍按旧频点返回。
        if let Some(old_freq) = user_freq(&state, &sender)
            && old_freq != req.freq
            && let Some(old_room) = state.freqs.get_mut(&old_freq)
        {
            old_room.members.remove(&sender);
        }
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
    // 通知同 freq 其他成员有人加入。
    let who = user_part(&sender);
    let others: Vec<String> = state
        .freqs
        .get(&req.freq)
        .map(|r| r.members.keys().filter(|u| **u != sender).cloned().collect())
        .unwrap_or_default();
    for u in &others {
        crate::notify(
            u,
            crate::i18n::t("srv.notify.joined")
                .replace("{freq}", &req.freq)
                .replace("{who}", &who)
                .to_string(),
        );
    }
    println!("[srv-standard] {} joined freq `{}` (members={})", who, req.freq, count);
    JoinFreqResponse {
        status: S::FreqIdle as i32,
        message: crate::i18n::t("srv.freq.joined")
            .replace("{freq}", &req.freq)
            .replace("{count}", &count.to_string())
            .to_string(),
    }
}

pub(crate) fn handle_leave_freq(
    sender: String,
    _req: protos::utils::empty::Empty,
) -> protos::frequency::basic::LeaveFreqResponse {
    use protos::frequency::basic::{LeaveFreqResponse, LeaveResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return LeaveFreqResponse {
                result: R::LeaveNotJoined as i32,
                message: crate::i18n::t("srv.common.not_in_freq").to_string(),
            };
        }
    };
    // 创建者不能直接离开（除非 -n/--no-mod 无版主频点）。
    let no_mod = state.freqs.get(&freq).map(|f| f.no_mod).unwrap_or(false);
    if state.freqs.get(&freq).map(|f| f.creator == sender).unwrap_or(false) && !no_mod {
        return LeaveFreqResponse {
            result: R::LeaveFailed as i32,
            message: crate::i18n::t("srv.freq.creator_leave").to_string(),
        };
    }
    if let Some(room) = state.freqs.get_mut(&freq) {
        room.members.remove(&sender);
    }
    state.users.get_mut(&sender).unwrap().freq = String::new();
    // 通知同 freq 剩余成员有人离开。
    let who = user_part(&sender);
    let others: Vec<String> = state
        .freqs
        .get(&freq)
        .map(|r| r.members.keys().filter(|u| **u != sender).cloned().collect())
        .unwrap_or_default();
    for u in &others {
        crate::notify(
            u,
            crate::i18n::t("srv.notify.left")
                .replace("{freq}", &freq)
                .replace("{who}", &who)
                .to_string(),
        );
    }
    // 无版主频点的创建者离开后，若不再有人则自动解散。
    if no_mod && state.freqs.get(&freq).map(|f| f.members.is_empty()).unwrap_or(false) {
        state.freqs.remove(&freq);
        VFS.lock().unwrap().remove(&freq);
        println!("[srv-standard] freq `{freq}` auto-disbanded (no_mod creator left)");
        return LeaveFreqResponse {
            result: R::LeaveSuccess as i32,
            message: crate::i18n::t("srv.freq.left_auto_disband")
                .replace("{freq}", &freq)
                .to_string(),
        };
    }
    println!("[srv-standard] {} left freq `{}`", user_part(&sender), freq);
    LeaveFreqResponse {
        result: R::LeaveSuccess as i32,
        message: crate::i18n::t("srv.freq.left").replace("{freq}", &freq).to_string(),
    }
}

pub(crate) fn handle_freq_info(
    sender: String,
    req: protos::frequency::basic::FreqInfoRequest,
) -> protos::frequency::basic::FreqInfoResponse {
    use protos::frequency::basic::{FreqInfo, FreqInfoResponse, MemberInfo};
    let state = STATE.lock().unwrap();
    // freq 为空时默认调用者当前所在 freq。
    let freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    match state.freqs.get(&freq) {
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
            FreqInfoResponse {
                found: true,
                info: Some(info),
                message: crate::i18n::t("srv.common.found").to_string(),
            }
        }
        None => FreqInfoResponse {
            found: false,
            info: None,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &req.freq).to_string(),
        },
    }
}

pub(crate) fn handle_disband_freq(
    sender: String,
    req: protos::frequency::mod_ops::DisbandRequest,
) -> protos::frequency::mod_ops::DisbandResponse {
    use protos::frequency::mod_ops::{DisbandResponse, DisbandResult as R};
    let mut state = STATE.lock().unwrap();
    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return DisbandResponse {
                    result: R::DisbandNotFound as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
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
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
        }
    };
    if room.creator != sender {
        return DisbandResponse {
            result: R::DisbandAccessDenied as i32,
            message: crate::i18n::t("srv.freq.disband_only_creator").to_string(),
        };
    }
    if room.members.len() > 1 && !req.force {
        return DisbandResponse {
            result: R::DisbandHasMembers as i32,
            message: crate::i18n::t("srv.freq.has_members")
                .replace("{freq}", &freq)
                .replace("{count}", &room.members.len().to_string())
                .to_string(),
        };
    }
    // 收集成员（用于清空状态与通知），随后解散。
    let members: Vec<String> = room.members.keys().cloned().collect();
    state.freqs.remove(&freq);
    for m in &members {
        if let Some(u) = state.users.get_mut(m) {
            u.freq = String::new();
        }
    }
    // 通知被解散的成员。
    for m in &members {
        if m != &sender {
            crate::notify(
                m,
                crate::i18n::t("srv.notify.disbanded")
                    .replace("{freq}", &freq)
                    .replace("{who}", &user_part(&sender))
                    .to_string(),
            );
        }
    }
    // 频点销毁时同时销毁其共享 VFS（freq 内共享，无私有文件系统）。
    VFS.lock().unwrap().remove(&freq);
    println!("[srv-standard] freq `{freq}` disbanded by {}", user_part(&sender));
    DisbandResponse {
        result: R::DisbandSuccess as i32,
        message: crate::i18n::t("srv.freq.disbanded").replace("{freq}", &freq).to_string(),
    }
}

pub(crate) fn handle_active_freqs(
    _sender: String,
    _req: protos::frequency::status::ActiveFreqsRequest,
) -> protos::frequency::status::ActiveFreqsResponse {
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
        message: crate::i18n::t("srv.freq.active_count")
            .replace("{count}", &state.freqs.len().to_string())
            .to_string(),
    }
}

pub(crate) fn handle_active_users(
    sender: String,
    req: protos::frequency::status::ActiveUsersRequest,
) -> protos::frequency::status::ActiveUsersResponse {
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
                    message: crate::i18n::t("srv.common.not_in_freq_specify").to_string(),
                };
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
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
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
        message: crate::i18n::t("srv.freq.member_count")
            .replace("{count}", &room.members.len().to_string())
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_moderators(
    sender: String,
    req: protos::frequency::status::ModeratorsRequest,
) -> protos::frequency::status::ModeratorsResponse {
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
                    message: crate::i18n::t("srv.common.not_in_freq_specify").to_string(),
                };
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
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
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
        message: crate::i18n::t("srv.freq.moderator_count")
            .replace("{count}", &n.to_string())
            .replace("{freq}", &freq)
            .to_string(),
    }
}
