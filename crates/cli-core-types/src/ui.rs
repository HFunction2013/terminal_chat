//! 宿主 UI 注册模型的跨 FFI 类别常量。
//!
//! 受当前 safer_ffi 版本限制，ReprC 结构体内不能内嵌 `Vec`（`String` 为 Opaque），
//! 因此注册表的条目结构保留在 cli-core 内部；跨 FFI 注册原语只传标量 / 字符串，
//! 查询经 `Out<Vec<String>>` 出参回传扁平记录行。本模块只承载动作 / 补全类别的序号。

/// 底部状态栏片段逐帧回调：返回当前帧要追加的片段文本；空串表示本帧无片段。
/// 未注册回调的插件不参与逐帧查询，避免无谓调用。
pub type BottomFragmentFn = unsafe extern "C" fn() -> safer_ffi::String;

/// 菜单项动作类别：直接执行命令行 / 弹出参数表单。
pub mod ui_action_kind {
    pub const RUN: i32 = 0;
    pub const FORM: i32 = 1;
}

/// 表单字段补全类别（对齐宿主 `FieldKind` 序号）。
pub mod ui_field_kind {
    pub const PLAIN: i32 = 0;
    pub const MEMBER: i32 = 1;
    pub const REMOTE_PATH: i32 = 2;
    pub const LOCAL_PATH: i32 = 3;
    pub const OPTION_KEY: i32 = 4;
    pub const SUBCOMMAND: i32 = 5;
    pub const PLUGIN: i32 = 6;
    pub const NAMESPACE: i32 = 7;
    pub const NO_COMPLETION: i32 = 8;
}
