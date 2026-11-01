//! //! 消息：send / reply / call@user

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

/// call 冷却：同一用户两次 call 之间至少间隔的秒数（now_ts 以秒计）。
const CALL_COOLDOWN_SECS: u64 = 3;
static CALL_COOLDOWNS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, resolve_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_send_message(sender: String, req: protos::messaging::message::SendMessageRequest) -> protos::messaging::message::SendMessageResponse {
    use protos::messaging::message::{SendMessageResponse, SendMessageResult as R};
    let mut state = STATE.lock().unwrap();
    ensure_user(&mut state, &sender);
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return SendMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: "you are not in any freq; use `join_freq` or `create_freq` first".into(),
            }
        }
    };
    // 被禁言检查。
    if let Some(member) = state.freqs.get(&freq).and_then(|f| f.members.get(&sender))
        && member.muted_until > now_ts()
    {
        return SendMessageResponse {
            result: R::SendMessageMuted as i32,
            message: "you are muted in this freq".into(),
        };
    }
    let targets: Vec<String> = req.users.iter().map(|u| u.to_string()).collect();
    if targets.is_empty() {
        let who = display_user(&state, &freq, &sender);
        println!(
            "[srv-standard] [{freq}] {who}: {}",
            req.message
        );
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
                    }
                }
            }
        }
        let room = state.freqs.get(&freq).expect("freq exists");
        for t in &resolved {
            if !room.members.contains_key(t) {
                return SendMessageResponse {
                    result: R::SendMessageUserNotFound as i32,
                    message: format!("user `{t}` is not in freq `{freq}`"),
                };
            }
        }
        let who = display_user(&state, &freq, &sender);
        println!(
            "[srv-standard] [{freq}] {who} -> {}: {}",
            resolved.join(","),
            req.message
        );
        // 通知每个定向目标（在线的用户稍后由 cli 轮询拉取）。
        for t in &resolved {
            if t.as_str() != sender {
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

pub(crate) fn handle_reply_message(sender: String, req: protos::messaging::message::ReplyMessageRequest) -> protos::messaging::message::ReplyMessageResponse {
    use protos::messaging::message::{ReplyMessageResponse, SendMessageResult as R};
    let state = STATE.lock().unwrap();
    let freq = match user_freq(&state, &sender) {
        Some(f) => f,
        None => {
            return ReplyMessageResponse {
                result: R::SendMessageNotInFreq as i32,
                message: "you are not in any freq".into(),
            }
        }
    };
    let target = match state.last_speaker.get(&freq) {
        Some(t) if t.as_str() != sender => t.clone(),
        _ => {
            return ReplyMessageResponse {
                result: R::SendMessageError as i32,
                message: "no one to reply to in this freq".into(),
            }
        }
    };
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!(
        "[srv-standard] [{freq}] {who} -> {target_disp}: {}",
        req.message
    );
    ReplyMessageResponse {
        result: R::SendMessageSuccess as i32,
        message: format!("-> {target_disp}: {}", req.message),
    }
}

pub(crate) fn handle_call_user(sender: String, req: protos::call::CallRequest) -> protos::call::CallResponse {
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
                    message: format!("call cooldown: try again in {}s", left),
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
                message: "call only works inside a freq; use `join_freq` first".into(),
            }
        }
    };
    if !req.freq.is_empty() && req.freq != scope {
        return CallResponse {
            result: R::CallError as i32,
            message: format!("you can only call inside your current freq `{scope}`"),
        };
    }
    let target = match resolve_user(&state, &scope, &req.user) {
        Ok(t) => t,
        Err(e) => {
            return CallResponse {
                result: R::CallUserNotFound as i32,
                message: e,
            }
        }
    };
    // 不能呼叫自己。
    if target == sender {
        return CallResponse {
            result: R::CallError as i32,
            message: "you cannot call yourself".into(),
        };
    }
    // 目标必须存在且在线。
    let profile = match state.users.get(&target) {
        Some(u) => u,
        None => {
            return CallResponse {
                result: R::CallUserNotFound as i32,
                message: format!("user `{target}` not found"),
            }
        }
    };
    if !profile.online {
        return CallResponse {
            result: R::CallUserOffline as i32,
            message: format!("user `{target}` is offline"),
        };
    }
    if !profile.away.is_empty() {
        return CallResponse {
            result: R::CallUserBusy as i32,
            message: format!("user `{target}` is {} (auto-reply: contact me later)", profile.away),
        };
    }
    let freq = profile.freq.clone();
    let who = display_user(&state, &freq, &sender);
    let target_disp = display_user(&state, &freq, &target);
    println!(
        "[srv-standard] CALL {who} -> {target_disp} ({freq}) @{}: {}",
        req.freq,
        req.message
    );
    // 呼叫即通知目标用户（打印到服务端日志，等价于 @user 提醒）。
    let msg = if req.message.is_empty() {
        format!("[call] {who} is calling you in {freq} (ring ring)")
    } else {
        format!("[call] {who} @{freq}: {}", req.message)
    };
    println!("[srv-standard] @{target} -> {msg}");
    notify(&target, msg.clone());
    CallResponse {
        result: R::CallRinging as i32,
        message: format!("calling {target} ... {msg}"),
    }
}
