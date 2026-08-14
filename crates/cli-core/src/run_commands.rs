use cli_core_types::PluginMetadata;
use crate::plugins::{get_plugin_by_command_name, is_plugin_command_name_registered};
use cli_core_types::Result;
use crate::{set_in_cmd, set_interrupted};
use ::safer_ffi::prelude::*;
use hook_macro::register_hook;
use libloading::{Library, library_filename};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

struct CommandGuard;
impl Drop for CommandGuard {
    fn drop(&mut self) {
        set_in_cmd(false);
        set_interrupted(false);
    }
}

#[register_hook]
fn run_command_impl(args: &safer_ffi::Vec<safer_ffi::String>) -> Result {
    let mut args = args.clone();
    if !unsafe { is_plugin_command_name_registered(args.first().unwrap()) } {
        let full_args: Vec<safer_ffi::String> =
            std::iter::once("std".into()).chain(args.iter().map(|s| s.clone())).collect();

        args = safer_ffi::Vec::from(full_args);
    }
    let command_name = args.first().unwrap();
    let res = unsafe { get_plugin_by_command_name(command_name) };
    let boxed = match res {
        safer_ffi::option::TaggedOption::Some(x) => x,
        safer_ffi::option::TaggedOption::None => {
            return Result {
                code: 103,
                message: format!("Failed to get plugin name from command name").into(),
            };
        }
    };

    let metadata: PluginMetadata = (*boxed).clone();
    let plugin_name = metadata.name;
    let plugin_dylib_name = metadata.dylib_name;
    let lib = match unsafe { Library::new(library_filename(plugin_dylib_name.to_string())) } {
        Ok(lib) => lib,
        Err(_) => {
            // Err loading.
            return Result {
                code: 101,
                message: format!("Failed to load plugin {plugin_name} ({plugin_dylib_name})").into(),
            };
        }
    };
    let lib_run_command = unsafe {
        match lib.get::<unsafe extern "C" fn(&safer_ffi::Vec<safer_ffi::String>) -> Result>(
            b"run_command",
        ) {
            Ok(f) => f,
            Err(_) => {
                // Err loading.
                return Result {
                    code: 101,
                    message: format!("Failed to load symbol run_command").into(),
                };
            }
        }
    };
    set_in_cmd(true);
    let _guard = CommandGuard;
    unsafe { lib_run_command(&args.into()) }
}
