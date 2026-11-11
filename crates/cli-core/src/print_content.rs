use hook_macro::register_hook;
use safer_ffi::ffi_export;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 是否处于 TUI 模式。TUI 下命令输出写入内存缓冲，由界面层取回渲染；
/// 普通 REPL 下照常 `println!` 到终端。
static TUI_MODE: AtomicBool = AtomicBool::new(false);

/// TUI 模式下暂存命令输出的缓冲（宿主侧单实例）。
static OUTPUT_BUFFER: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[register_hook]
fn print_content(content: &safer_ffi::String) {
    if TUI_MODE.load(Ordering::SeqCst) {
        OUTPUT_BUFFER.lock().unwrap().push(content.to_string());
    } else {
        println!("{}", &**content);
    }
}

/// 切换输出模式：`true` 进入 TUI（输出入缓冲），`false` 回到 REPL（直接打印）。
#[ffi_export]
pub fn tc_cli_set_tui_mode(enabled: bool) {
    TUI_MODE.store(enabled, Ordering::SeqCst);
    if !enabled {
        OUTPUT_BUFFER.lock().unwrap().clear();
    }
}

/// 取走并清空 TUI 输出缓冲（命令同步输出）。
#[ffi_export]
pub fn tc_cli_output_drain() -> safer_ffi::Vec<safer_ffi::String> {
    let mut buf = OUTPUT_BUFFER.lock().unwrap();
    let items: Vec<String> = std::mem::take(&mut *buf);
    items.into_iter().map(|s| s.into()).collect::<Vec<_>>().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_switch_buffers_then_prints() {
        // TUI 模式：输出入缓冲，drain 取回。
        tc_cli_set_tui_mode(true);
        unsafe { print_content(&"hello line".into()) };
        let drained: Vec<safer_ffi::String> = tc_cli_output_drain().into();
        let texts: Vec<String> = drained.iter().map(|s| s.to_string()).collect();
        assert_eq!(texts, vec!["hello line".to_string()]);

        // 切回 REPL：缓冲被清空，后续输出不再入缓冲。
        tc_cli_set_tui_mode(false);
        unsafe { print_content(&"repl line".into()) };
        let drained: Vec<safer_ffi::String> = tc_cli_output_drain().into();
        assert!(drained.is_empty());
    }
}
