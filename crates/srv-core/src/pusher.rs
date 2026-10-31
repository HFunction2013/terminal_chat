//! pusher：服务端主动推送（通知实时下发到在线客户端）。
//!
//! srv 主程序为每个在线会话注册一个 `mpsc::Sender<Vec<u8>>`（写线程消费并写帧），
//! srv-standard 的 `notify()` 把通知编码为 `notification_push` 帧字节后，
//! 经 `tc_srv_pusher_push` 投递给目标用户（user@ip）的推送通道。
//! 目标不在线 / 通道断开时 `push` 返回 0，通知仍保留在 NOTIFICATIONS 队列兜底。

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::mpsc::Sender;
use std::sync::{LazyLock, Mutex};

/// 在线用户（user@ip）→ 推送 Sender 指针（`Box::into_raw`；unregister 时释放）。
static PUSHERS: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 注册推送通道。`tx` 必须是 `Box::into_raw(Box::new(mpsc::Sender<Vec<u8>>))`。
/// 覆盖同名用户时释放旧 Sender。成功返回 1。
#[unsafe(no_mangle)]
pub extern "C" fn tc_srv_pusher_register(user: *const c_char, tx: *mut c_void) -> i32 {
    if user.is_null() || tx.is_null() {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(user) }.to_string_lossy().into_owned();
    let mut map = PUSHERS.lock().unwrap();
    if let Some(old) = map.insert(name, tx as usize) {
        unsafe { drop(Box::from_raw(old as *mut Sender<Vec<u8>>)) };
    }
    1
}

/// 移除并释放推送通道。成功返回 1。
#[unsafe(no_mangle)]
pub extern "C" fn tc_srv_pusher_unregister(user: *const c_char) -> i32 {
    if user.is_null() {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(user) }.to_string_lossy().into_owned();
    let mut map = PUSHERS.lock().unwrap();
    match map.remove(&name) {
        Some(p) => {
            unsafe { drop(Box::from_raw(p as *mut Sender<Vec<u8>>)) };
            1
        }
        None => 0,
    }
}

/// 向在线用户投递一帧推送字节（非阻塞）。投递成功返回 1，离线/失败返回 0。
#[unsafe(no_mangle)]
pub extern "C" fn tc_srv_pusher_push(user: *const c_char, bytes: *const u8, len: u32) -> i32 {
    if user.is_null() {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(user) }.to_string_lossy().into_owned();
    let map = PUSHERS.lock().unwrap();
    if let Some(&p) = map.get(&name) {
        let data = if bytes.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec()
        };
        let tx: &Sender<Vec<u8>> = unsafe { &*((p as usize) as *const Sender<Vec<u8>>) };
        if tx.send(data).is_ok() {
            return 1;
        }
    }
    0
}
