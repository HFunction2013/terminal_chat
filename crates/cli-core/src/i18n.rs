//! 运行时 i18n 词典与语言解析。
//!
//! 插件在初始化阶段经 `register_i18n_entry` 上报编译期词典条目（key/en/zh）；
//! UI 注册表只存 i18n key，查询时按当前语言现译，不在注册时固化译文快照。
//!
//! 语言来源：全局选项 `LANGUAGE` 优先（core 内部直读，不经 FFI）；选项缺失或
//! 无法解析时回退进程环境变量（`LANG` → `LC_ALL`），再回退英文。

use ::safer_ffi::prelude::*;
use cli_core_types::Result;
use hook_macro::register_hook;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// 支持的语言（内部枚举；序号经 FFI 对外暴露：En=0，Zh=1）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Lang {
    En,
    Zh,
}

/// 运行时词典条目。
struct DictEntry {
    owner: String,
    en: String,
    zh: String,
}

static DICT: LazyLock<Mutex<HashMap<String, DictEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 解析语言值：去首尾空白并小写；空串 → `None`；`zh*` → `Zh`；其余 → `En`。
fn parse_value(v: &str) -> Option<Lang> {
    let v = v.trim().to_ascii_lowercase();
    if v.is_empty() {
        return None;
    }
    Some(if v.starts_with("zh") { Lang::Zh } else { Lang::En })
}

/// 进程环境语言（OnceLock 缓存）：`LANG` → `LC_ALL`；非中文变体回退 `En`。
fn env_lang() -> Lang {
    static ENV: LazyLock<Lang> = LazyLock::new(|| {
        for var in ["LANG", "LC_ALL"] {
            if let Ok(v) = std::env::var(var)
                && let Some(lang) = parse_value(&v)
            {
                return lang;
            }
        }
        Lang::En
    });
    *ENV
}

/// 当前语言：每次现场解析，不做 OnceLock 缓存；全局选项 `LANGUAGE` 优先，
/// 缺失或无法解析时回退 [`env_lang`]。
pub(crate) fn current_lang() -> Lang {
    if let Some(v) = crate::global_settings::raw_option("LANGUAGE")
        && let Some(lang) = parse_value(&v)
    {
        return lang;
    }
    env_lang()
}

/// 上报一条词典条目；同 owner+key 覆盖；owner 为空串记为 `"host"`。
pub(crate) fn register_entry(owner: &str, key: &str, en: &str, zh: &str) {
    let owner = if owner.is_empty() { "host" } else { owner };
    let mut dict = DICT.lock().unwrap();
    dict.insert(
        key.to_string(),
        DictEntry { owner: owner.to_string(), en: en.to_string(), zh: zh.to_string() },
    );
}

/// 按当前语言翻译；未找到返回 key 本身；`Zh` 下 zh 为空回退 en。
pub(crate) fn translate(key: &str) -> String {
    let dict = DICT.lock().unwrap();
    let Some(entry) = dict.get(key) else {
        return key.to_string();
    };
    match current_lang() {
        Lang::En => entry.en.clone(),
        Lang::Zh => {
            if entry.zh.is_empty() {
                entry.en.clone()
            } else {
                entry.zh.clone()
            }
        }
    }
}

/// 移除归属为 `owner` 的全部词典条目（卸载插件时调用）。
pub(crate) fn unregister_by_owner(owner: &str) {
    DICT.lock().unwrap().retain(|_, e| e.owner != owner);
}

/// 清空全部词典条目（清空插件时调用）。
pub(crate) fn clear_all() {
    DICT.lock().unwrap().clear();
}

/// 上报一条词典条目；归属取当前初始化插件上下文，空归属记为 `host`。
#[register_hook]
fn register_i18n_entry(
    key: &safer_ffi::String,
    en: &safer_ffi::String,
    zh: &safer_ffi::String,
) -> Result {
    let owner = crate::plugins::current_owner().unwrap_or_default();
    register_entry(&owner, &key.to_string(), &en.to_string(), &zh.to_string());
    Result::success()
}

/// 按当前语言查询译文；未知 key 原样回传。
#[register_hook]
fn lookup_i18n(key: &safer_ffi::String, out: Out<'_, safer_ffi::String>) -> Result {
    out.write(translate(&key.to_string()).into());
    Result::success()
}

/// 当前语言序号：`En=0`，`Zh=1`。
#[register_hook]
fn get_current_lang(out: Out<'_, i32>) -> Result {
    let ord = match current_lang() {
        Lang::En => 0,
        Lang::Zh => 1,
    };
    out.write(ord);
    Result::success()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    #[test]
    fn parse_value_recognizes_chinese_aliases() {
        for zh in ["zh", "zh-cn", "zh-hans", "zh-Hans", "zh_CN.UTF-8", "ZH"] {
            assert_eq!(parse_value(zh), Some(Lang::Zh), "{zh}");
        }
        for en in ["en", "en_US", "fr", "de_DE.UTF-8"] {
            assert_eq!(parse_value(en), Some(Lang::En), "{en}");
        }
        assert_eq!(parse_value(""), None);
        assert_eq!(parse_value("   "), None);
    }

    #[test]
    fn register_and_translate_picks_language_with_fallbacks() {
        clear_all();
        // 显式切到英文，避免受进程环境影响。
        set_language("en");
        register_entry("t-owner-a", "ui.hello", "Hello", "你好");
        register_entry("t-owner-b", "ui.missing_zh", "OnlyEn", "");

        assert_eq!(translate("ui.hello"), "Hello");
        assert_eq!(translate("ui.missing_zh"), "OnlyEn");
        assert_eq!(translate("ui.unknown"), "ui.unknown");

        set_language("zh");
        assert_eq!(translate("ui.hello"), "你好");
        // 缺 zh 回退 en。
        assert_eq!(translate("ui.missing_zh"), "OnlyEn");
        assert_eq!(translate("ui.unknown"), "ui.unknown");

        remove_language();
        clear_all();
    }

    #[test]
    fn empty_owner_falls_back_to_host() {
        clear_all();
        register_entry("", "ui.host", "Host", "宿主");
        let dict = DICT.lock().unwrap();
        assert_eq!(dict.get("ui.host").unwrap().owner, "host");
        drop(dict);
        clear_all();
    }

    #[test]
    fn unregister_by_owner_removes_only_owner_entries() {
        clear_all();
        register_entry("owner-x", "k.x", "X", "甲");
        register_entry("owner-y", "k.y", "Y", "乙");

        unregister_by_owner("owner-x");
        let dict = DICT.lock().unwrap();
        assert!(!dict.contains_key("k.x"));
        assert!(dict.contains_key("k.y"));
        assert_eq!(dict.get("k.y").unwrap().owner, "owner-y");
        drop(dict);

        clear_all();
        assert!(DICT.lock().unwrap().is_empty());
    }

    #[test]
    fn current_lang_prefers_language_option_then_env() {
        // 选项缺失时回退 env_lang()（不断言具体取值，避免依赖进程环境）。
        remove_language();
        assert_eq!(current_lang(), env_lang());

        set_language("zh");
        assert_eq!(current_lang(), Lang::Zh);
        set_language("zh-Hans");
        assert_eq!(current_lang(), Lang::Zh);
        set_language("en_US");
        assert_eq!(current_lang(), Lang::En);

        remove_language();
        assert_eq!(current_lang(), env_lang());
        clear_all();
    }

    #[test]
    fn lookup_hook_round_trips_translation() {
        clear_all();
        set_language("en");
        register_entry("t-hook", "ui.hook", "Hooked", "钩子");

        let mut buf: ManuallyDrop<safer_ffi::String> =
            ManuallyDrop::new(safer_ffi::String::from(String::new()));
        unsafe {
            let _ = lookup_i18n(&"ui.hook".into(), Out::from(&mut buf));
        }
        assert_eq!(ManuallyDrop::into_inner(buf).to_string(), "Hooked");

        remove_language();
        clear_all();
    }

    fn set_language(value: &str) {
        crate::global_settings::set_raw_option("LANGUAGE", value);
    }

    fn remove_language() {
        crate::global_settings::remove_raw_option("LANGUAGE");
    }
}
