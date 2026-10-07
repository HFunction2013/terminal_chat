//! srv-core：terminal_chat 服务端核心库（cdylib）。
//!
//! 仿照 cli-core 的架构，为服务端提供：
//! - 插件加载（`plugins`）：`load_plugin` 加载 srv-standard 等服务端插件；
//! - 频道注册（`channels`）：注册/查询频道（含通话频道），绑定 channel handler；
//! - 包处理（`packets`）：`on_recieve_packet` 接收数据包并按频道路由分发；
//! - 会话管理（`sessions`）：客户端会话骨架；
//! - 输出（`print_content`）：服务端日志输出。
//!
//! 所有对外函数经 `safer-ffi` / `hook_macro` 导出为 C ABI，
//! srv 主程序与插件均通过 `libloading` 动态加载并调用。

mod channels;
mod packets;
mod plugins;
mod print_content;
mod sessions;
