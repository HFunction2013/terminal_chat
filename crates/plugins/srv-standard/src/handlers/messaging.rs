//! //! 消息：send / reply / call@user

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

/// call 冷却：同一用户两次 call 之间至少间隔的秒数（now_ts 以秒计）。
const CALL_COOLDOWN_SECS: u64 = 3;
static CALL_COOLDOWNS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
#[allow(unused_imports)]
use crate::{
    FreqRoom, Member, NOTIFICATIONS, STATE, ServerState, UserProfile, VFS, VfsEntry, VfsUser,
    display_user, ensure_user, mod_auth, norm_path, notify, now_ts, resolve_user, user_freq,
    user_part, valid_freq, vfs_ensure_dirs,
};

/// 接收者是否忽略了发送者（ignore 语义：被忽略者的一切交互不再触达忽略者）。
fn ignores(state: &crate::ServerState, receiver: &str, sender: &str) -> bool {
    state.users.get(receiver).map(|u| u.ignored.contains(sender)).unwrap_or(false)
}

pub(crate) fn handle_send_message(
    sender: String,
    req: protos::messaging::message::SendMessageRequest,
) -> protos::messaging::message::SendMessageResponse {
    use protos::messaging::message::{SendMessageResponse, SendMessageResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return SendMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: crate::i18n::t("srv.msg.not_in_freq").to_string(),
            };
        }
    };
    // 被禁言检查。
    if let Some(member) = state.freqs.get(&freq).and_then(|f| f.members.get(&sender))
        && member.muted_until > now_ts()
    {
        return SendMessageResponse {
            result: R::SendMessageMuted as i32,
            message: crate::i18n::t("srv.msg.muted").to_string(),
        };
    }
    let targets: Vec<String> = req.users.iter().map(|u| u.to_string()).collect();
    if targets.is_empty() {
        let who = display_user(&state, &freq, &sender);
        println!("[srv-standard] [{freq}] {who}: {}", req.message);
        // 广播：把消息推送给该 freq 内除发送者以外的所有成员（原实现漏了这一步，
        // 导致别的成员有时收不到 msg）。
        let members: Vec<String> =
            state.freqs.get(&freq).map(|r| r.members.keys().cloned().collect()).unwrap_or_default();
        for m in &members {
            if m != &sender && !ignores(&state, m, &sender) {
                notify(m, format!("[msg:{freq}] {who}: {}", req.message));
            }
        }
        state.last_speaker.insert(freq.clone(), sender.clone());
        SendMessageResponse {
            result: R::SendMessageSuccess as i32,
            message: format!("[{freq}] {who}: {}", req.message),
        }
    } else {
        // 定向发送：先解析目标（freq 内用户名唯一时允许省略 @IP）。
        let mut resolved: Vec<String> = Vec::new();
        for t in &targets {
            match resolve_user(&state, &freq, t) {
                Ok(u) => resolved.push(u),
                Err(e) => {
                    return SendMessageResponse {
                        result: R::SendMessageUserNotFound as i32,
                        message: e,
                    };
                }
            }
        }
        let room = state.freqs.get(&freq).expect("freq exists");
        for t in &resolved {
            if !room.members.contains_key(t) {
                return SendMessageResponse {
                    result: R::SendMessageUserNotFound as i32,
                    message: crate::i18n::t("srv.msg.target_not_in_freq")
                        .replace("{target}", t)
                        .replace("{freq}", &freq)
                        .to_string(),
                };
            }
        }
        let who = display_user(&state, &freq, &sender);
        println!("[srv-standard] [{freq}] {who} -> {}: {}", resolved.join(","), req.message);
        // 通知每个定向目标（在线的用户稍后由 cli 轮询拉取）。
        for t in &resolved {
            if t.as_str() != sender && !ignores(&state, t, &sender) {
                notify(t, format!("[msg:{freq}] {who}: {}", req.message));
            }
        }
        state.last_speaker.insert(freq, sender);
        SendMessageResponse {
            result: R::SendMessageSuccess as i32,
            message: format!("-> {}: {}", resolved.join(","), req.message),
        }
    }
}

pub(crate) fn handle_reply_message(
    sender: String,
    req: protos::messaging::message::ReplyMessageRequest,
) -> protos::messaging::message::ReplyMessageResponse {
    use protos::messaging::message::{ReplyMessageResponse, SendMessageResult as R};
    let state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return ReplyMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: crate::i18n::t("srv.common.not_in_freq").to_string(),
            };
        }
    };
    // 被禁言者在该 freq 内不能进行任何交互（reply 同样受限）。
    if let Some(member) = state.freqs.get(&freq).and_then(|f| f.members.get(&sender))
        && member.muted_until > now_ts()
    {
        return ReplyMessageResponse {
            result: R::SendMessageMuted as i32,
            message: crate::i18n::t("srv.msg.muted").to_string(),
        };
    }
    let target = match state.last_speaker.get(&freq) {
        Some(t) if t.as_str() != sender => t.clone(),
        _ => {
            return ReplyMessageResponse {
                result: R::SendMessageError as i32,
                message: crate::i18n::t("srv.msg.no_reply_target").to_string(),
            };
        }
    };
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!("[srv-standard] [{freq}] {who} -> {target_disp}: {}", req.message);
    ReplyMessageResponse {
        result: R::SendMessageSuccess as i32,
        message: format!("-> {target_disp}: {}", req.message),
    }
}

pub(crate) fn handle_call_user(
    sender: String,
    req: protos::call::CallRequest,
) -> protos::call::CallResponse {
    use protos::call::{CallResponse, CallResult as R};
    // call 冷却：上次 call 后 3 秒内拒绝。
    {
        let now = now_ts();
        let mut cd = CALL_COOLDOWNS.lock().unwrap();
        if let Some(&last) = cd.get(&sender) {
            let left = CALL_COOLDOWN_SECS.saturating_sub(now.saturating_sub(last));
            if left > 0 {
                return CallResponse {
                    result: R::CallError as i32,
                    message: crate::i18n::t("srv.msg.call_cooldown")
                        .replace("{left}", &left.to_string())
                        .to_string(),
                };
            }
        }
        cd.insert(sender.clone(), now);
    }
    let state = STATE.lock().unwrap();
    // call 只能在调用者当前所在的 freq 内使用。
    let scope = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return CallResponse {
                result: R::CallError as i32,
                message: crate::i18n::t("srv.msg.call_not_in_freq").to_string(),
            };
        }
    };
    // 被禁言者在该 freq 内不能进行任何交互（call 同样受限）。
    if let Some(member) = state.freqs.get(&scope).and_then(|f| f.members.get(&sender))
        && member.muted_until > now_ts()
    {
        return CallResponse {
            result: R::CallError as i32,
            message: crate::i18n::t("srv.msg.muted").to_string(),
        };
    }
    if !req.freq.is_empty() && req.freq != scope {
        return CallResponse {
            result: R::CallError as i32,
            message: crate::i18n::t("srv.msg.call_scope").replace("{freq}", &scope).to_string(),
        };
    }
    let target = match resolve_user(&state, &scope, &req.user) {
        Ok(t) => t,
        Err(e) => return CallResponse { result: R::CallUserNotFound as i32, message: e },
    };
    // 不能呼叫自己。
    if target == sender {
        return CallResponse {
            result: R::CallError as i32,
            message: crate::i18n::t("srv.msg.call_self").to_string(),
        };
    }
    // 目标必须存在且在线。
    let profile = match state.users.get(&target) {
        Some(u) => u,
        None => {
            return CallResponse {
                result: R::CallUserNotFound as i32,
                message: crate::i18n::t("srv.user.not_found")
                    .replace("{target}", &target)
                    .to_string(),
            };
        }
    };
    if !profile.online {
        return CallResponse {
            result: R::CallUserOffline as i32,
            message: crate::i18n::t("srv.msg.target_offline")
                .replace("{target}", &target)
                .to_string(),
        };
    }
    if !profile.away.is_empty() {
        return CallResponse {
            result: R::CallUserBusy as i32,
            message: crate::i18n::t("srv.msg.target_busy")
                .replace("{target}", &target)
                .replace("{away}", &profile.away)
                .to_string(),
        };
    }
    let freq = profile.freq.clone();
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!("[srv-standard] CALL {who} -> {target_disp} ({freq}) @{}: {}", req.freq, req.message);
    // 呼叫即通知目标用户（打印到服务端日志，等价于 @user 提醒）。
    let msg = if req.message.is_empty() {
        crate::i18n::t("srv.notify.call")
            .replace("{who}", &who)
            .replace("{freq}", &freq)
            .to_string()
    } else {
        // 除 [call] 通道标签外无可译散文：{who}/@{freq}/消息体均为数据，原样透传。
        format!("[call] {who} @{freq}: {}", req.message)
    };
    println!("[srv-standard] @{target} -> {msg}");
    notify(&target, msg.clone());
    CallResponse {
        result: R::CallRinging as i32,
        message: crate::i18n::t("srv.msg.calling")
            .replace("{target}", &target)
            .replace("{msg}", &msg)
            .to_string(),
    }
}
