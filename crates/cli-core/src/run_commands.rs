use crate::cmd_state::{set_in_cmd, set_interrupted};
use crate::plugins::{get_plugin_by_command_name, is_plugin_command_name_registered};
use ::safer_ffi::prelude::*;
use cli_core_types::PluginMetadata;
use cli_core_types::Result;
use cli_core_types::RunCommandFn;
use hook_macro::register_hook;
use libloading::Library;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use which_dylib::{FindError, FindLibBuilder};

struct CommandGuard;
impl Drop for CommandGuard {
    fn drop(&mut self) {
        set_in_cmd(false);
        set_interrupted(false);
    }
}

#[register_hook]
fn run_command(args: &safer_ffi::Vec<safer_ffi::String>) -> Result {
    let mut args = args.clone();
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
    match FindLibBuilder::new().find_result(&plugin_dylib_name) {
        Ok(plugin_lib) => {
            let plugin_lib_path =
                plugin_lib.to_str().expect("Failed to convert `PathBuf` to `&str`");
            let lib = match unsafe { Library::new(plugin_lib_path) } {
                Ok(lib) => lib,
                Err(_) => {
                    // Err loading.
                    return Result {
                        code: 101,
                        message: format!(
                            "Failed to load plugin {plugin_name} ({plugin_dylib_name})"
                        )
                        .into(),
                    };
                }
            };
            let lib_run_command = unsafe {
                match lib.get::<RunCommandFn>(b"run_command") {
                    Ok(f) => f,
                    Err(_) => {
                        // Err loading.
                        return Result {
                            code: 101,
                            message: "Failed to load symbol run_command".to_string().into(),
                        };
                    }
                }
            };
            set_in_cmd(true);
            let _guard = CommandGuard;
            lib_run_command(&args)
        }
        Err(e) => match e {
            FindError::NotFound(s) => {
                Result { code: 100, message: format!("NotFoundError: {}", s).into() }
            }
            FindError::Ambiguous(v) => {
                Result { code: 99, message: format!("AmbiguousError: {:?}", v).into() }
            }
        },
    }
}
