use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use hook_macro::register_hook;
use safer_ffi::option::TaggedOption;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 当前所在频点运行时态；`None` 表示未加入频点。
///
/// 由宿主在频点变化时经 [`set_current_freq`] 写入，供插件底部状态栏回调
/// 经 [`get_current_freq`] 读取（选项类状态仍走 `get_global_option` /
/// `get_session_option`）。
static CURRENT_FREQ: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

/// 宿主写入当前频点；空串视为已离开频点（`None`）。
#[register_hook]
fn set_current_freq(freq: &safer_ffi::String) -> Result {
    let mut slot = CURRENT_FREQ.lock().unwrap();
    *slot = if freq.trim().is_empty() { None } else { Some(freq.to_string()) };
    Result::success()
}

/// 读取当前频点；未加入频点时回传 `None`。
#[register_hook]
fn get_current_freq(out: Out<'_, TaggedOption<safer_ffi::String>>) -> Result {
    let slot = CURRENT_FREQ.lock().unwrap();
    let value = slot.clone().map(|v| v.into()).into();
    out.write(value);
    Result::success()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn read_freq() -> Option<String> {
        let mut buf: ManuallyDrop<TaggedOption<safer_ffi::String>> =
            ManuallyDrop::new(TaggedOption::None);
        unsafe {
            let _ = get_current_freq(Out::from(&mut buf));
        }
        match ManuallyDrop::into_inner(buf) {
            TaggedOption::Some(v) => Some(v.to_string()),
            TaggedOption::None => None,
        }
    }

    #[test]
    fn freq_slot_round_trips() {
        unsafe {
            let _ = set_current_freq(&"lobby".into());
        }
        assert_eq!(read_freq(), Some("lobby".to_string()));

        unsafe {
            let _ = set_current_freq(&"".into());
        }
        assert_eq!(read_freq(), None);
    }
}
