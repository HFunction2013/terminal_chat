//! srv：terminal_chat 服务端启动器（二进制）。
//!
//! 启动流程：
//! 1. 用 `which_dylib` 定位并加载 `srv_core`（libsrv_core.so）；
//! 2. 通过 srv-core 的 `load_plugin` 默认加载 `srv_standard`
//!    （libsrv_standard.so），由它注册通话/聊天等频道及 handler；
//! 3. 进入 stdin 投递循环：把每行输入模拟成一个 `Packet` 交给
//!    `on_recieve_packet` 分发，便于手工验证频道注册与包路由。
//!
//! 注意：传输层（TCP/WebSocket）与真实网络协议尚未接入，
//! 当前 stdin 循环只是骨架演示，后续替换为真实连接即可。

use anyhow::{Result, anyhow};
use libloading::{Library, Symbol};
use srv_core_types::channels::ChannelMetadata;
use srv_core_types::packets::Packet;
use srv_core_types::{
    GetAllChannelsFn, LoadPluginFn, OnRecievePacketFn,
};
use std::io::{self, BufRead};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use which_dylib::{FindError, FindLibBuilder};

static SRV_CORE: OnceLock<Library> = OnceLock::new();

fn get_srv_core() -> &'static Library {
    match FindLibBuilder::new().find_result("srv_core") {
        Ok(s) => {
            let lib_path = s.to_str().expect("Failed to convert `PathBuf` to `&str`");

            SRV_CORE.get_or_init(|| unsafe {
                Library::new(lib_path)
                    .unwrap_or_else(|e| panic!("Failed to load library '{}': {}", lib_path, e))
            })
        }
        Err(e) => match e {
            FindError::NotFound(s) => panic!("NotFoundError: {}", s),
            FindError::Ambiguous(v) => panic!("AmbiguousError: {:?}", v),
        },
    }
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn main() -> Result<()> {
    // 获取 srv-core 符号。
    let load_plugin: Symbol<LoadPluginFn> = unsafe { get_srv_core().get(b"load_plugin") }
        .map_err(|e| anyhow!("Failed to find symbol 'load_plugin': {e}"))?;
    let on_recieve_packet: Symbol<OnRecievePacketFn> =
        unsafe { get_srv_core().get(b"on_recieve_packet") }
            .map_err(|e| anyhow!("Failed to find symbol 'on_recieve_packet': {e}"))?;
    let get_all_channels: Symbol<GetAllChannelsFn> =
        unsafe { get_srv_core().get(b"get_all_channels") }
            .map_err(|e| anyhow!("Failed to find symbol 'get_all_channels': {e}"))?;

    // 默认加载 srv-standard。
    let res = load_plugin(&"srv_standard".into());
    if res.success != 1 {
        eprintln!(
            "Failed to load srv_standard: exit_code={} msg={:?}",
            res.exit_code, res.msg
        );
        std::process::exit(1);
    }
    println!("[srv] srv-standard loaded.");

    // 列出已注册频道。
    let channels: Vec<ChannelMetadata> = get_all_channels().into();
    let names: Vec<String> = channels.iter().map(|c| c.name.to_string()).collect();
    println!("[srv] registered channels: {:?}", names);

    println!("[srv] ready. Type `channel:payload` to simulate an incoming packet; `exit` to quit.");
    println!("[srv] e.g. `call:hello?` or `chat:hi everyone`");

    // 模拟收包循环（TODO(transport): 替换为真实网络连接）。
    let mut seq: u64 = 0;
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line == "exit" || line == "quit" {
            break;
        }
        let (channel, payload) = match line.split_once(':') {
            Some((c, p)) => (c.trim().to_string(), p.trim().to_string()),
            None => ("chat".to_string(), line.clone()),
        };
        seq += 1;
        let packet = Packet {
            channel: channel.into(),
            sender: "demo-client".into(),
            payload: payload.into(),
            timestamp: now_ts(),
            seq,
        };
        let res = on_recieve_packet(&packet);
        println!(
            "-> handled={} code={} msg={}",
            res.handled, res.code, res.message
        );
    }

    println!("[srv] bye.");
    Ok(())
}
