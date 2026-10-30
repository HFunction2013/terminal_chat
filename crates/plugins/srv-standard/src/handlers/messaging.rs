//! //! 消息：send / reply / call@user

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
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
    let state = STATE.lock().unwrap();
    // 解析目标（freq 内用户名唯一时允许省略 @IP；优先 req.freq，其次调用者所在 freq）。
    let scope = if req.freq.is_empty() {
        user_freq(&state, &sender).unwrap_or_default()
    } else {
        req.freq.clone()
    };
    let target = match resolve_user(&state, &scope, &req.user) {
        Ok(t) => t,
        Err(e) => {
            return CallResponse {
                result: R::CallUserNotFound as i32,
                message: e,
            }
        }
    };
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
