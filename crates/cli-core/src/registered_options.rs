//! 已注册选项（registered options）注册表。
//!
//! 插件在初始化阶段通过 `register_option` 登记自己接受的选项 key；
//! `set`/`setg` 写入前用 `option_is_registered` 校验，未注册 key 被拒绝；
//! `get_registered_options` 供 TUI / REPL 做选项 key 补全。
//!
//! key 统一按大写归一化存储与比较，注册与查询大小写不敏感。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// key（大写归一化）→ 归属插件名；owner 为空表示非插件来源（内置/遗留）。
static REGISTRY: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 归一化选项 key：去首尾空白并转大写。
fn normalize(key: &str) -> String {
    key.trim().to_uppercase()
}

/// 登记一个可接受的选项 key（重复登记幂等）。归属取当前初始化插件上下文。
#[register_hook]
fn register_option(key: &safer_ffi::String) -> Result {
    let owner = crate::plugins::current_owner().unwrap_or_default();
    let mut map = REGISTRY.lock().unwrap();
    map.insert(normalize(&key.to_string()), owner);
    Result::success()
}

/// 移除归属为 `owner` 的全部选项 key（卸载插件时调用）。
pub(crate) fn unregister_by_owner(owner: &str) {
    REGISTRY.lock().unwrap().retain(|_, o| o != owner);
}

/// 清空全部已注册选项（清空插件时调用）。
pub(crate) fn clear_all() {
    REGISTRY.lock().unwrap().clear();
}

/// 判断选项 key 是否已注册（成功=已注册，错误=未注册）。
#[register_hook]
fn option_is_registered(key: &safer_ffi::String) -> Result {
    let map = REGISTRY.lock().unwrap();
    if map.contains_key(&normalize(&key.to_string())) {
        Result::success()
    } else {
        Result::error("option not registered")
    }
}

/// 列出全部已注册选项 key（供补全）。
#[register_hook]
fn get_registered_options(out: Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>) -> Result {
    let map = REGISTRY.lock().unwrap();
    let mut keys: Vec<String> = map.iter().map(|(k, _)| k.clone()).collect();
    keys.sort();
    let vec: safer_ffi::vec::Vec<safer_ffi::String> =
        keys.into_iter().map(|k| k.into()).collect::<Vec<_>>().into();
    out.write(vec);
    Result::success()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    #[test]
    fn register_and_lookup_normalizes_uppercase() {
        // 以小写 + 空白注册，查询时按大写归一化仍应命中。
        unsafe {
            let _ = register_option(&"  test_option_reg  ".into());
            assert_eq!(option_is_registered(&"TEST_OPTION_REG".into()).code, 0);
            assert_eq!(option_is_registered(&"test_option_reg".into()).code, 0);
        }
    }

    #[test]
    fn unregistered_key_lookup_fails() {
        unsafe {
            assert_ne!(option_is_registered(&"DEFINITELY_NOT_REGISTERED_XYZ".into()).code, 0);
        }
    }

    #[test]
    fn registered_options_list_contains_registered_key() {
        unsafe {
            let _ = register_option(&"TEST_OPTION_LIST".into());
            let mut buf: ManuallyDrop<safer_ffi::vec::Vec<safer_ffi::String>> =
                ManuallyDrop::new(safer_ffi::vec::Vec::from(std::vec::Vec::new()));
            let _ = get_registered_options(Out::from(&mut buf));
            let keys: Vec<String> =
                ManuallyDrop::into_inner(buf).iter().map(|s| s.to_string()).collect();
            assert!(keys.iter().any(|k| k == "TEST_OPTION_LIST"));
        }
    }

    #[test]
    fn unregister_by_owner_removes_only_owner_entries() {
        unsafe {
            crate::plugins::set_current_owner(Some("plug-a".to_string()));
            let _ = register_option(&"OWNER_OPT_A".into());
            crate::plugins::set_current_owner(Some("plug-b".to_string()));
            let _ = register_option(&"OWNER_OPT_B".into());
            crate::plugins::set_current_owner(None);

            unregister_by_owner("plug-a");
            assert_ne!(option_is_registered(&"OWNER_OPT_A".into()).code, 0);
            assert_eq!(option_is_registered(&"OWNER_OPT_B".into()).code, 0);
            unregister_by_owner("plug-b");
            assert_ne!(option_is_registered(&"OWNER_OPT_B".into()).code, 0);
        }
    }
}
