//! //! 会话：status（ping）/ route / notifications

#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_status(sender: String, req: protos::connection::session::StatusRequest) -> protos::connection::session::StatusResponse {
    use protos::connection::session::StatusResponse;
    StatusResponse {
        nonce: req.nonce,
        rtt_ms: 1,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        message: format!("pong from srv-standard (hi {sender})"),
    }
}

pub(crate) fn handle_route(_sender: String, _req: protos::connection::session::RouteRequest) -> protos::connection::session::RouteResponse {
    use protos::connection::session::RouteResponse;
    RouteResponse {
        server: "srv-standard".to_string(),
        hops: vec![],
        success: true,
        message: "direct connection (1 hop)".to_string(),
    }
}


pub(crate) fn handle_heartbeat(sender: String, req: protos::connection::session::HeartbeatRequest) -> protos::connection::session::HeartbeatResponse {
    use protos::connection::session::HeartbeatResponse;
    use protos::frequency::notification::NotificationMessage;
    // RTT：客户端上报 Unix 毫秒时钟，服务端与本地时钟差值（正数才有效）。
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let rtt = if req.client_ts > 0 { (now_ms - req.client_ts).max(0) } else { 0 };
    // 长连接心跳包顺带把待推送通知带回去（取出即清空，语义与 notifications RPC 一致）。
    let now = now_ts();
    let mut all = NOTIFICATIONS.lock().unwrap();
    let items = all
        .remove(&sender)
        .unwrap_or_default()
        .into_iter()
        .map(|text| {
            let (kind, rest) = text.split_once(':').unwrap_or(("msg", text.as_str()));
            NotificationMessage {
                kind: kind.trim_start_matches('[').to_string(),
                from: String::new(),
                text: rest.to_string(),
                at: now as i64,
            }
        })
        .collect();
    HeartbeatResponse {
        rtt_ms: rtt,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        notifications: items,
        message: "pong".into(),
    }
}

pub(crate) fn handle_notifications(sender: String, _req: protos::frequency::notification::NotificationsRequest) -> protos::frequency::notification::NotificationsResponse {
    use protos::frequency::notification::{NotificationMessage, NotificationsResponse};
    let now = now_ts();
    let mut all = NOTIFICATIONS.lock().unwrap();
    let items = all
        .remove(&sender)
        .unwrap_or_default()
        .into_iter()
        .map(|text| {
            let (kind, rest) = text.split_once(':').unwrap_or(("msg", text.as_str()));
            NotificationMessage {
                kind: kind.trim_start_matches('[').to_string(),
                from: String::new(),
                text: rest.to_string(),
                at: now as i64,
            }
        })
        .collect();
    NotificationsResponse {
        success: true,
        items,
        message: "notifications".into(),
    }
}
