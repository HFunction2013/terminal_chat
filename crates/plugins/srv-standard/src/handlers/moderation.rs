//! //! 频点管理：kick / ban / unban / mute / unmute / op / deop / trop

#[allow(unused_imports)]
use crate::{
    FreqRoom, Member, NOTIFICATIONS, STATE, ServerState, UserProfile, VFS, VfsEntry, VfsUser,
    display_user, ensure_user, mod_auth, norm_path, notify, now_ts, resolve_user, user_freq,
    user_part, valid_freq, vfs_ensure_dirs,
};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

pub(crate) fn handle_kick(
    sender: String,
    req: protos::frequency::mod_ops::KickRequest,
) -> protos::frequency::mod_ops::KickResponse {
    use protos::frequency::mod_ops::{KickResponse, KickResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
                return KickResponse {
                    result: R::KickError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    if !state.freqs.contains_key(&freq) {
        return KickResponse {
            result: R::KickUserNotFound as i32,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
        };
    }
    if !mod_auth(&state, &freq, &sender) {
        return KickResponse {
            result: R::KickAccessDenied as i32,
            message: crate::i18n::t("srv.mod.not_moderator").to_string(),
        };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == target {
            return KickResponse {
                result: R::KickCannotKickCreator as i32,
                message: crate::i18n::t("srv.mod.cannot_kick_creator").to_string(),
            };
        }
        if room.members.remove(&target).is_none() {
            return KickResponse {
                result: R::KickUserNotFound as i32,
                message: crate::i18n::t("srv.mod.target_not_in_freq")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    }
    state.users.get_mut(&target).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} kicked {}", user_part(&sender), user_part(&target));
    notify(
        &target,
        crate::i18n::t("srv.notify.kicked")
            .replace("{freq}", &freq)
            .replace("{who}", &display_user(&state, &freq, &sender))
            .to_string(),
    );
    KickResponse {
        result: R::KickSuccess as i32,
        message: crate::i18n::t("srv.mod.kicked")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_ban(
    sender: String,
    req: protos::frequency::mod_ops::BanRequest,
) -> protos::frequency::mod_ops::BanResponse {
    use protos::frequency::mod_ops::{BanResponse, BanResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return BanResponse {
                    result: R::BanError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    if !state.freqs.contains_key(&freq) {
        return BanResponse {
            result: R::BanUserNotFound as i32,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
        };
    }
    if !mod_auth(&state, &freq, &sender) {
        return BanResponse {
            result: R::BanAccessDenied as i32,
            message: crate::i18n::t("srv.mod.not_moderator").to_string(),
        };
    }
    if !state.users.contains_key(&target) {
        return BanResponse {
            result: R::BanUserNotFound as i32,
            message: crate::i18n::t("srv.user.not_found").replace("{target}", &target).to_string(),
        };
    }
    {
        let room = state.freqs.get_mut(&freq).expect("checked above");
        if room.creator == target {
            return BanResponse {
                result: R::BanCannotBanCreator as i32,
                message: crate::i18n::t("srv.mod.cannot_ban_creator").to_string(),
            };
        }
        room.banned.insert(target.clone());
        room.members.remove(&target);
    }
    state.users.get_mut(&target).map(|u| u.freq = String::new());
    println!("[srv-standard] {freq}: {} banned {}", user_part(&sender), user_part(&target));
    notify(
        &target,
        crate::i18n::t("srv.notify.banned")
            .replace("{freq}", &freq)
            .replace("{who}", &display_user(&state, &freq, &sender))
            .to_string(),
    );
    BanResponse {
        result: R::BanSuccess as i32,
        message: crate::i18n::t("srv.mod.banned")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_unban(
    sender: String,
    req: protos::frequency::mod_ops::UnbanRequest,
) -> protos::frequency::mod_ops::UnbanResponse {
    use protos::frequency::mod_ops::{UnbanResponse, UnbanResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return UnbanResponse {
                    result: R::UnbanError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    if !state.freqs.contains_key(&freq) {
        return UnbanResponse {
            result: R::UnbanUserNotFound as i32,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
        };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnbanResponse {
            result: R::UnbanAccessDenied as i32,
            message: crate::i18n::t("srv.mod.not_moderator").to_string(),
        };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.banned.remove(&target) {
        UnbanResponse {
            result: R::UnbanSuccess as i32,
            message: crate::i18n::t("srv.mod.unbanned")
                .replace("{target}", &target)
                .replace("{freq}", &freq)
                .to_string(),
        }
    } else {
        UnbanResponse {
            result: R::UnbanUserNotFound as i32,
            message: crate::i18n::t("srv.mod.not_banned").replace("{target}", &target).to_string(),
        }
    }
}

pub(crate) fn handle_mute(
    sender: String,
    req: protos::frequency::mod_ops::MuteRequest,
) -> protos::frequency::mod_ops::MuteResponse {
    use protos::frequency::mod_ops::{MuteResponse, MuteResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return MuteResponse {
                    result: R::MuteError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    if !state.freqs.contains_key(&freq) {
        return MuteResponse {
            result: R::MuteUserNotFound as i32,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
        };
    }
    if !mod_auth(&state, &freq, &sender) {
        return MuteResponse {
            result: R::MuteAccessDenied as i32,
            message: crate::i18n::t("srv.mod.not_moderator").to_string(),
        };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    if room.creator == target {
        return MuteResponse {
            result: R::MuteCannotMuteCreator as i32,
            message: crate::i18n::t("srv.mod.cannot_mute_creator").to_string(),
        };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => {
            return MuteResponse {
                result: R::MuteUserNotFound as i32,
                message: crate::i18n::t("srv.mod.target_not_in_freq")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    };
    member.muted_until = match req.duration {
        Some(d) if d >= 0 => now_ts() + d as u64,
        _ => u64::MAX, // -1 / 缺省：永久
    };
    println!("[srv-standard] {freq}: {} muted {}", user_part(&sender), user_part(&target));
    notify(
        &target,
        crate::i18n::t("srv.notify.muted")
            .replace("{freq}", &freq)
            .replace("{who}", &display_user(&state, &freq, &sender))
            .to_string(),
    );
    MuteResponse {
        result: R::MuteSuccess as i32,
        message: crate::i18n::t("srv.mod.muted")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_unmute(
    sender: String,
    req: protos::frequency::mod_ops::UnmuteRequest,
) -> protos::frequency::mod_ops::UnmuteResponse {
    use protos::frequency::mod_ops::{UnmuteResponse, UnmuteResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return UnmuteResponse {
                    result: R::UnmuteError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    if !state.freqs.contains_key(&freq) {
        return UnmuteResponse {
            result: R::UnmuteError as i32,
            message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
        };
    }
    if !mod_auth(&state, &freq, &sender) {
        return UnmuteResponse {
            result: R::UnmuteAccessDenied as i32,
            message: crate::i18n::t("srv.mod.not_moderator").to_string(),
        };
    }
    let room = state.freqs.get_mut(&freq).expect("checked above");
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => {
            return UnmuteResponse {
                result: R::UnmuteUserNotMuted as i32,
                message: crate::i18n::t("srv.mod.target_not_in_freq")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    };
    if member.muted_until == 0 {
        return UnmuteResponse {
            result: R::UnmuteUserNotMuted as i32,
            message: crate::i18n::t("srv.mod.not_muted").replace("{target}", &target).to_string(),
        };
    }
    member.muted_until = 0;
    UnmuteResponse {
        result: R::UnmuteSuccess as i32,
        message: crate::i18n::t("srv.mod.unmuted")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_op(
    sender: String,
    req: protos::frequency::mod_ops::OpRequest,
) -> protos::frequency::mod_ops::OpResponse {
    use protos::frequency::mod_ops::{OpResponse, OpResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return OpResponse {
                    result: R::OpError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => {
            return OpResponse {
                result: R::OpUserNotFound as i32,
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
        }
    };
    // 只有创建者可授权。
    if room.creator != sender {
        return OpResponse {
            result: R::OpAccessDenied as i32,
            message: crate::i18n::t("srv.mod.only_creator_op").to_string(),
        };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => {
            return OpResponse {
                result: R::OpUserNotFound as i32,
                message: crate::i18n::t("srv.mod.target_not_in_freq")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    };
    if member.is_moderator {
        return OpResponse {
            result: R::OpAlreadyModerator as i32,
            message: crate::i18n::t("srv.mod.already_moderator")
                .replace("{target}", &target)
                .to_string(),
        };
    }
    member.is_moderator = true;
    println!("[srv-standard] {freq}: {} opped {}", user_part(&sender), user_part(&target));
    OpResponse {
        result: R::OpSuccess as i32,
        message: crate::i18n::t("srv.mod.opped")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_deop(
    sender: String,
    req: protos::frequency::mod_ops::DeopRequest,
) -> protos::frequency::mod_ops::DeopResponse {
    use protos::frequency::mod_ops::{DeopResponse, DeopResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return DeopResponse {
                    result: R::DeopError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => {
            return DeopResponse {
                result: R::DeopUserNotFound as i32,
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
        }
    };
    if room.creator != sender {
        return DeopResponse {
            result: R::DeopAccessDenied as i32,
            message: crate::i18n::t("srv.mod.only_creator_deop").to_string(),
        };
    }
    if room.creator == target {
        return DeopResponse {
            result: R::DeopCannotDeopCreator as i32,
            message: crate::i18n::t("srv.mod.cannot_deop_creator").to_string(),
        };
    }
    let member = match room.members.get_mut(&target) {
        Some(m) => m,
        None => {
            return DeopResponse {
                result: R::DeopUserNotFound as i32,
                message: crate::i18n::t("srv.mod.target_not_in_freq")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    };
    if !member.is_moderator {
        return DeopResponse {
            result: R::DeopNotModerator as i32,
            message: crate::i18n::t("srv.mod.not_moderator_target")
                .replace("{target}", &target)
                .to_string(),
        };
    }
    member.is_moderator = false;
    DeopResponse {
        result: R::DeopSuccess as i32,
        message: crate::i18n::t("srv.mod.deopped")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}

pub(crate) fn handle_trop(
    sender: String,
    req: protos::frequency::mod_ops::TropRequest,
) -> protos::frequency::mod_ops::TropResponse {
    use protos::frequency::mod_ops::{TropResponse, TropResult as R};
    let mut state = STATE.lock().unwrap(); // 解析目标用户（freq 内用户名唯一时允许省略 @IP）。
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
            None => {
                return TropResponse {
                    result: R::TropError as i32,
                    message: crate::i18n::t("srv.common.not_in_freq").to_string(),
                };
            }
        }
    } else {
        req.freq.clone()
    };
    let room = match state.freqs.get_mut(&freq) {
        Some(r) => r,
        None => {
            return TropResponse {
                result: R::TropUserNotFound as i32,
                message: crate::i18n::t("srv.freq.not_found").replace("{freq}", &freq).to_string(),
            };
        }
    };
    if room.creator != sender {
        return TropResponse {
            result: R::TropAccessDenied as i32,
            message: crate::i18n::t("srv.mod.only_creator_trop").to_string(),
        };
    }
    if room.creator == target {
        return TropResponse {
            result: R::TropSelfTransfer as i32,
            message: crate::i18n::t("srv.mod.self_transfer").to_string(),
        };
    }
    if !room.members.contains_key(&target) {
        return TropResponse {
            result: R::TropUserNotFound as i32,
            message: crate::i18n::t("srv.mod.target_not_in_freq")
                .replace("{target}", &target)
                .to_string(),
        };
    }
    room.creator = target.clone();
    if let Some(m) = room.members.get_mut(&target) {
        m.is_moderator = true;
    }
    println!("[srv-standard] {freq}: creator transferred to {}", user_part(&target));
    TropResponse {
        result: R::TropSuccess as i32,
        message: crate::i18n::t("srv.mod.trop_success")
            .replace("{target}", &target)
            .replace("{freq}", &freq)
            .to_string(),
    }
}
