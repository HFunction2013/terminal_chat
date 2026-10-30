//! //! 频点管理：kick / ban / unban / mute / unmute / op / deop / trop

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, resolve_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_kick(sender: String, req: protos::frequency::mod_ops::KickRequest) -> protos::frequency::mod_ops::KickResponse {
    use protos::frequency::mod_ops::{KickResponse, KickResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return KickResponse { result: R::KickUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => {
                return KickResponse { result: R::KickError as i32, message: "you are not in any freq".into() }
            }
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return KickResponse { result: R::KickUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return KickResponse { result: R::KickAccessDenied as i32, message: "you are not a moderator".into() };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == target {
            return KickResponse { result: R::KickCannotKickCreator as i32, message: "cannot kick the creator".into() };
        }
        if room.members.remove(&target).is_none() {
            return KickResponse { result: R::KickUserNotFound as i32, message: format!("user `{}` not in freq", target) };
        }
    }
    state.users.get_mut(&target).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} kicked {}", user_part(&sender), user_part(&target));
    notify(&target, format!("[kick:{freq}] you were kicked by {}", display_user(&state, &freq, &sender)));
    KickResponse { result: R::KickSuccess as i32, message: format!("kicked {} from {freq}", target) }
}

pub(crate) fn handle_ban(sender: String, req: protos::frequency::mod_ops::BanRequest) -> protos::frequency::mod_ops::BanResponse {
    use protos::frequency::mod_ops::{BanResponse, BanResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return BanResponse { result: R::BanUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return BanResponse { result: R::BanError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return BanResponse { result: R::BanUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return BanResponse { result: R::BanAccessDenied as i32, message: "you are not a moderator".into() };
    }
    if !state.users.contains_key(&target) {
        return BanResponse { result: R::BanUserNotFound as i32, message: format!("user `{}` not found", target) };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == target {
            return BanResponse { result: R::BanCannotBanCreator as i32, message: "cannot ban the creator".into() };
        }
        room.banned.insert(target.clone());
        room.members.remove(&target);
    }
    state.users.get_mut(&target).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} banned {}", user_part(&sender), user_part(&target));
    notify(&target, format!("[ban:{freq}] you were banned by {}", display_user(&state, &freq, &sender)));
    BanResponse { result: R::BanSuccess as i32, message: format!("banned {} from {freq}", target) }
}

pub(crate) fn handle_unban(sender: String, req: protos::frequency::mod_ops::UnbanRequest) -> protos::frequency::mod_ops::UnbanResponse {
    use protos::frequency::mod_ops::{UnbanResponse, UnbanResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return UnbanResponse { result: R::UnbanUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return UnbanResponse { result: R::UnbanError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return UnbanResponse { result: R::UnbanUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnbanResponse { result: R::UnbanAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.banned.remove(&target) {
        UnbanResponse { result: R::UnbanSuccess as i32, message: format!("unbanned {} from {freq}", target) }
    } else {
        UnbanResponse { result: R::UnbanUserNotFound as i32, message: format!("user `{}` is not banned", target) }
    }
}

pub(crate) fn handle_mute(sender: String, req: protos::frequency::mod_ops::MuteRequest) -> protos::frequency::mod_ops::MuteResponse {
    use protos::frequency::mod_ops::{MuteResponse, MuteResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return MuteResponse { result: R::MuteUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return MuteResponse { result: R::MuteError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return MuteResponse { result: R::MuteUserNotFound as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return MuteResponse { result: R::MuteAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.creator == target {
        return MuteResponse { result: R::MuteCannotMuteCreator as i32, message: "cannot mute the creator".into() };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => return MuteResponse { result: R::MuteUserNotFound as i32, message: format!("user `{}` not in freq", target) },
    };
    member.muted_until = match req.duration {
        Some(d) if d >= 0 => now_ts() + d as u64,
        _ => u64::MAX, // -1 / 缺省：永久
    };
    println!("[srv-standard] {freq}: {} muted {}", user_part(&sender), user_part(&target));
    notify(&target, format!("[mute:{freq}] you were muted by {}", display_user(&state, &freq, &sender)));
    MuteResponse { result: R::MuteSuccess as i32, message: format!("muted {} in {freq}", target) }
}

pub(crate) fn handle_unmute(sender: String, req: protos::frequency::mod_ops::UnmuteRequest) -> protos::frequency::mod_ops::UnmuteResponse {
    use protos::frequency::mod_ops::{UnmuteResponse, UnmuteResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return UnmuteResponse { result: R::UnmuteUserNotMuted as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return UnmuteResponse { result: R::UnmuteError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    if !state.freqs.contains_key(&freq) {
        return UnmuteResponse { result: R::UnmuteError as i32, message: format!("freq `{freq}` not found") };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnmuteResponse { result: R::UnmuteAccessDenied as i32, message: "you are not a moderator".into() };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => return UnmuteResponse { result: R::UnmuteUserNotMuted as i32, message: format!("user `{}` not in freq", target) },
    };
    if member.muted_until == 0 {
        return UnmuteResponse { result: R::UnmuteUserNotMuted as i32, message: format!("user `{}` is not muted", target) };
    }
    member.muted_until = 0;
    UnmuteResponse { result: R::UnmuteSuccess as i32, message: format!("unmuted {} in {freq}", target) }
}

pub(crate) fn handle_op(sender: String, req: protos::frequency::mod_ops::OpRequest) -> protos::frequency::mod_ops::OpResponse {
    use protos::frequency::mod_ops::{OpResponse, OpResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return OpResponse { result: R::OpUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return OpResponse { result: R::OpError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return OpResponse { result: R::OpUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    // 只有创建者可授权。
    if room.creator != sender {
        return OpResponse { result: R::OpAccessDenied as i32, message: "only the creator can grant mod".into() };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => return OpResponse { result: R::OpUserNotFound as i32, message: format!("user `{}` not in freq", target) },
    };
    if member.is_moderator {
        return OpResponse { result: R::OpAlreadyModerator as i32, message: format!("{} is already a moderator", target) };
    }
    member.is_moderator = true;
    println!("[srv-standard] {freq}: {} opped {}", user_part(&sender), user_part(&target));
    OpResponse { result: R::OpSuccess as i32, message: format!("{} is now a moderator of {freq}", target) }
}

pub(crate) fn handle_deop(sender: String, req: protos::frequency::mod_ops::DeopRequest) -> protos::frequency::mod_ops::DeopResponse {
    use protos::frequency::mod_ops::{DeopResponse, DeopResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return DeopResponse { result: R::DeopUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return DeopResponse { result: R::DeopError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return DeopResponse { result: R::DeopUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    if room.creator != sender {
        return DeopResponse { result: R::DeopAccessDenied as i32, message: "only the creator can revoke mod".into() };
    }
    if room.creator == target {
        return DeopResponse { result: R::DeopCannotDeopCreator as i32, message: "cannot deop the creator".into() };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => return DeopResponse { result: R::DeopUserNotFound as i32, message: format!("user `{}` not in freq", target) },
    };
    if !member.is_moderator {
        return DeopResponse { result: R::DeopNotModerator as i32, message: format!("{} is not a moderator", target) };
    }
    member.is_moderator = false;
    DeopResponse { result: R::DeopSuccess as i32, message: format!("{} is no longer a moderator of {freq}", target) }
}

pub(crate) fn handle_trop(sender: String, req: protos::frequency::mod_ops::TropRequest) -> protos::frequency::mod_ops::TropResponse {
    use protos::frequency::mod_ops::{TropResponse, TropResult as R};
    let mut state = STATE.lock().unwrap();    // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
    let scope_freq = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope_freq, &req.user) {
        Ok(t) => t,
        Err(e) => return TropResponse { result: R::TropUserNotFound as i32, message: e },
    };

    let freq = if req.freq.is_empty() {
        match user_freq(&state, &sender) {
            Some(f) => f,
            None => return TropResponse { result: R::TropError as i32, message: "you are not in any freq".into() },
        }
    } else { req.freq.clone() };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => return TropResponse { result: R::TropUserNotFound as i32, message: format!("freq `{freq}` not found") },
    };
    if room.creator != sender {
        return TropResponse { result: R::TropAccessDenied as i32, message: "only the creator can transfer ownership".into() };
    }
    if room.creator == target {
        return TropResponse { result: R::TropSelfTransfer as i32, message: "you are already the creator".into() };
    }
    if !room.members.contains_key(&target) {
        return TropResponse { result: R::TropUserNotFound as i32, message: format!("user `{}` not in freq", target) };
    }
    room.creator = target.clone();
    if let Some(m) = room.members.get_mut(&target) {
        m.is_moderator = true;
    }
    println!("[srv-standard] {freq}: creator transferred to {}", user_part(&target));
    TropResponse { result: R::TropSuccess as i32, message: format!("{} is now the creator of {freq}", target) }
}
