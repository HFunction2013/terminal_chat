//! 通知轮询：REPL 每轮调用一次，拉取服务器端待读通知并显示。
//! 未连接 / 未设 USERNAME / 无通知时全部静默（见 cli-standard notifications 命令）。

use cli_core_types::RunCommandFn;

pub fn poll(run_command: &RunCommandFn) {
    let args: Vec<String> = vec!["notifications".to_string()];
    let args_ffi: safer_ffi::Vec<safer_ffi::String> =
        args.into_iter().map(|s| s.into()).collect::<Vec<_>>().into();
    let _ = run_command(&args_ffi);
}
