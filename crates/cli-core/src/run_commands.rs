use crate::aliases::get_alias;
use crate::cmd_state::{set_in_cmd, set_interrupted};
use crate::plugins::{
    get_plugin_by_command_name, get_plugin_library, is_plugin_command_name_registered,
};
use ::safer_ffi::prelude::*;
use cli_core_types::PluginMetadata;
use cli_core_types::Result;
use cli_core_types::RunCommandFn;
use hook_macro::register_hook;
use libloading::Library;
use safer_ffi::option::TaggedOption;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use which_dylib::{FindError, FindLibBuilder};

/// 别名展开的最大深度，防止 alias 互相指向形成死循环。
const MAX_ALIAS_EXPANSION_DEPTH: usize = 10;

struct CommandGuard;
impl Drop for CommandGuard {
    fn drop(&mut self) {
        set_in_cmd(false);
        set_interrupted(false);
    }
}

/// 展开命令行开头的别名（可含参数、可链式指向别名）。
///
/// 例如别名 `ll -> ls -la`：输入 `ll /tmp` 展开为 `ls -la /tmp`。
/// 深度超过 [`MAX_ALIAS_EXPANSION_DEPTH`] 时停止，避免环。
fn expand_aliases(args: &mut Vec<safer_ffi::String>) {
    for _ in 0..MAX_ALIAS_EXPANSION_DEPTH {
        let Some(first) = args.first().cloned() else { return };
        // `get_alias` 是 register_hook 生成的 unsafe wrapper。
        let target = match unsafe { get_alias(&first) } {
            TaggedOption::Some(t) => t.to_string(),
            TaggedOption::None => return,
        };
        // target 可能是 `cmd` 或 `cmd arg1 arg2`，按 shell 词法拆分。
        let mut expanded: Vec<safer_ffi::String> = shell_words::split(&target)
            .unwrap_or_else(|_| vec![target])
            .into_iter()
            .map(|s| s.into())
            .collect();
        expanded.extend(args.iter().skip(1).cloned());
        *args = expanded;
    }
}

#[register_hook]
fn run_command(args: &safer_ffi::Vec<safer_ffi::String>) -> Result {
    let mut args = args.clone();
    // 先做别名展开，再判断命令名是否已注册（否则 `ll` 这类别名会被加 std 前缀）。
    let mut args_vec: Vec<safer_ffi::String> = args.iter().cloned().collect();
    expand_aliases(&mut args_vec);
    args = safer_ffi::Vec::from(args_vec);

    if !unsafe { is_plugin_command_name_registered(args.first().unwrap()) } {
        let full_args: Vec<safer_ffi::String> =
            std::iter::once("std".into()).chain(args.iter().cloned()).collect();

        args = safer_ffi::Vec::from(full_args);
    }
    let command_name = args.first().unwrap();
    let res = unsafe { get_plugin_by_command_name(command_name) };
    let boxed = match res {
        safer_ffi::option::TaggedOption::Some(x) => x,
        safer_ffi::option::TaggedOption::None => {
            return Result {
                code: 103,
                message: "Failed to get plugin name from command name".to_string().into(),
            };
        }
    };

    let metadata: PluginMetadata = (*boxed).clone();
    let plugin_name = metadata.name;
    let plugin_dylib_name = metadata.dylib_name;

    // 优先复用 `load_plugin` 保留的句柄：Windows 上重复 `Library::new`
    // 每次 LoadLibrary 都是新句柄，插件内 static（cli-standard 的 LIB）
    // 会重新初始化，导致 "cli-core not initialized"。复用同一句柄跨平台一致。
    let lib_run_command: RunCommandFn =
        if let Some(plugin_lib) = get_plugin_library(&plugin_name.to_string()) {
            match unsafe { plugin_lib.get::<RunCommandFn>(b"run_command") } {
                Ok(f) => *f,
                Err(_) => {
                    return Result {
                        code: 101,
                        message: "Failed to load symbol run_command".to_string().into(),
                    };
                }
            }
        } else {
            // 回退：未通过 load_plugin 注册的插件，按路径临时加载。
            match FindLibBuilder::new().find_result(&plugin_dylib_name) {
                Ok(plugin_lib) => {
                    let plugin_lib_path =
                        plugin_lib.to_str().expect("Failed to convert `PathBuf` to `&str`");
                    let lib = match unsafe { Library::new(plugin_lib_path) } {
                        Ok(lib) => lib,
                        Err(_) => {
                            return Result {
                                code: 101,
                                message: format!(
                                    "Failed to load plugin {plugin_name} ({plugin_dylib_name})"
                                )
                                .into(),
                            };
                        }
                    };
                    match unsafe { lib.get::<RunCommandFn>(b"run_command") } {
                        Ok(f) => *f,
                        Err(_) => {
                            return Result {
                                code: 101,
                                message: "Failed to load symbol run_command".to_string().into(),
                            };
                        }
                    }
                }
                Err(e) => {
                    return match e {
                        FindError::NotFound(s) => {
                            Result { code: 100, message: format!("NotFoundError: {}", s).into() }
                        }
                        FindError::Ambiguous(v) => {
                            Result { code: 99, message: format!("AmbiguousError: {:?}", v).into() }
                        }
                    };
                }
            }
        };

    set_in_cmd(true);
    let _guard = CommandGuard;
    lib_run_command(&args)
}
