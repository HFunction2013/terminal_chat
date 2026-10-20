use safer_ffi::prelude::*;

/// 一个命令别名：`name` 是别名，`target` 是被别名的原命令（可含参数）。
///
/// 例如 `create alias "ls -la" ll` 产生 `AliasDef { name: "ll", target: "ls -la" }`，
/// 之后输入 `ll` 会被展开为 `ls -la` 再执行。
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AliasDef {
    /// 别名（展开时匹配的第一个 token）。
    pub name: safer_ffi::String,
    /// 目标命令，可以是 `cmd` 或 `cmd arg1 arg2` 形式。
    pub target: safer_ffi::String,
}

impl AliasDef {
    pub fn new(name: &str, target: &str) -> Self {
        AliasDef { name: name.into(), target: target.into() }
    }
}

impl Default for AliasDef {
    fn default() -> Self {
        AliasDef { name: "".into(), target: "".into() }
    }
}
