# Plugin FFI Contract

This page describes the contract a plugin must satisfy: the exported entry functions, the metadata struct, naming conventions, and the load/unload lifecycle.

## How a plugin is loaded

1. The host (`cli` or `srv`) loads its core library (`cli-core` / `srv-core`) through `libloading`.
2. A plugin is located by its dylib name (resolved by `which_dylib`), opened as a `cdylib`, and its entry symbols are resolved.
3. The host calls `get_plugin_metadata` to read the plugin's `PluginMetadata`, checks the declared host version range, then calls `on_init_plugin`.
4. In `on_init_plugin`, the plugin opens the core library path passed in the host metadata and resolves core hook symbols; it registers its commands, options, and UI.
5. Commands dispatched to the plugin run through `run_command` (cli) or a registered channel handler (srv).
6. On unload the host calls `on_shutdown_plugin`, then removes the plugin's metadata, namespace, and every option/UI entry it registered. Ownership is tracked by an internal core context (no owner parameter is added to the FFI registration primitives). The library handle is kept for the process lifetime; it is not unloaded while executing.

## Command routing (cli)

A plugin's `command_name` is its namespace. At load, core parses the top-level command names from the plugin `command_yaml` and registers them in a global command table. On dispatch the first token is matched against:

1. an explicit namespace (`command_name`), then
2. the global command table, which resolves the owning plugin for an unqualified command.

Core supplies the namespace token internally when invoking the plugin's `run_command`; the user command line is not rewritten with a hard-coded default namespace. An unresolved command reports not-found.

The `load namespace` / `unload namespace` commands are the `command_name`-keyed view of the same plugin set that `load plugin` / `unload plugin` key by plugin name; both sides resolve a plugin through its `command_name` metadata, not through a separate text manifest.

## Client plugin entry functions

Declared in `crates/cli-core-types/src/plugin_fn_types.rs`. Export with `#[ffi_export]` from `safer_ffi`.

| Exported symbol | Signature | Purpose |
| --- | --- | --- |
| `get_plugin_metadata` | `extern "C" fn(Out<'_, PluginMetadata>) -> Result` | Write the plugin metadata into the host-provided output buffer |
| `on_init_plugin` | `extern "C" fn(HostMetadata) -> PluginResult` | Initialize: open the core library, register options/UI, wire up state |
| `on_shutdown_plugin` | `extern "C" fn() -> PluginResult` | Release plugin-owned resources; core state is owned by the core library |
| `run_command` | `extern "C" fn(&safer_ffi::Vec<safer_ffi::String>) -> Result` | Dispatch one command; `args[0]` is the command/namespace name |
| `cli_take_pushes` | `unsafe extern "C" fn() -> safer_ffi::Vec<safer_ffi::String>` | Drain the async push queue (QUIET-aware) for the host to display |
| `list_remote_dir` | `extern "C" fn(&safer_ffi::String, Out<'_, Vec<String>>) -> Result` | List remote directory entry names for TUI path completion; empty list on failure |
| `is_connected` | `extern "C" fn() -> i32` | Report current TCP connection state: `1` connected, `0` not connected |
| `tc_complete_arg` (optional) | `extern "C" fn(&Vec<String>, i32, &String, Out<'_, Vec<String>>) -> Result` | Return completion candidates for a plugin-defined concept. `argv` is the split command line, `arg_index` is the 1-based argument position, `prefix` is the token being typed; write candidate strings to the output buffer. Only invoked for argument types core does not recognize |

`list_remote_dir` and `is_connected` are expected from the standard client plugin; the host treats a missing optional symbol gracefully where applicable. `tc_complete_arg` is a best-effort completion hook: if the symbol is absent or returns an empty list, the host falls back to free-text / no completion without blocking input.

## Argument completion: built-in types vs custom concepts

The command tree in `command_yaml` may declare a completion `type` per argument. Core maps each declared type to a built-in completion category the host fills from its own snapshots and registries:

| Declared type(s) | Built-in category | Host candidate source |
| --- | --- | --- |
| `MEMBER` | Member | Sidebar member snapshot |
| `REMOTE_PATH` / `REMOTE` | Remote path | Plugin `list_remote_dir` RPC listing |
| `LOCAL_PATH` / `LOCAL` | Local path | Local filesystem |
| `OPTION_KEY` / `OPTION` | Option key | Registered option keys |
| `SUBCOMMAND` | Subcommand | Registered subcommands of the parent command |
| `PLUGIN` | Plugin | Loaded plugin names |
| `NAMESPACE` | Namespace | Loaded plugin namespaces |
| `PLAIN` / `TEXT` / `STRING` / unset | Free text | No semantic completion |
| `NO_COMPLETION` / `NONE` | No completion | Empty candidate list |

A type name core does not recognize (for example `USER`) is a **custom concept**: core keeps the raw type name, the host routes that argument position to the owning plugin's optional `tc_complete_arg` callback, and the plugin supplies candidates. Built-in types never reach this callback.

Arguments declared as options (`short`/`long` present, or an `action` such as `set_true`) are flags/valued options and do not occupy a positional argument slot. The positional completion index counts only positional arguments, so placing a flag before a positional argument does not shift the positional argument's declared `type`.

Form fields registered via `register_form_field` use the same built-in category numbers. A field's `kind` must match the completion `type` of the corresponding argument in the command's `command_yaml` — for example the option-key field of `set`/`get`/`setg`/`getg`/`unset`/`unsetg` must be `OPTION_KEY`. A mismatch (such as declaring the key field `PLAIN`) makes the host dialog treat it as free text and offer no option-key candidates.

A command entry marked `hidden: true` is still dispatched normally by the plugin, but it is skipped when core builds the completion candidate table, so it does not appear in command or subcommand suggestions.

## Server plugin entry functions

Declared in `crates/srv-core-types/src/plugin_fn_types.rs` and `plugins.rs`.

| Exported symbol | Signature | Purpose |
| --- | --- | --- |
| `get_plugin_metadata` | `extern "C" fn(Out<'_, SrvPluginMetadata>) -> SrvResult` | Write the server plugin metadata |
| `on_init_plugin` | `extern "C" fn(SrvHostMetadata) -> SrvPluginResult` | Open `srv-core`, register channels and their handlers |
| `on_shutdown_plugin` | `extern "C" fn() -> SrvPluginResult` | Release plugin-owned state |

Server plugins do not implement `run_command`; instead they register a channel handler per RPC. See [hooks.md](hooks.md#server-hooks).

## `PluginMetadata` fields

Defined in `crates/cli-core-types/src/plugins.rs` (`SrvPluginMetadata` is the symmetric server version, without `command_yaml`/`command_name`).

| Field | Type | Meaning |
| --- | --- | --- |
| `command_yaml` | `safer_ffi::String` | YAML describing the command tree: name, args, subcommands, aliases, help. Embed via `include_str!("../commands.yaml")` |
| `name` | `safer_ffi::String` | Plugin display name, also the registry key (e.g. `"cli-standard"`) |
| `dylib_name` | `safer_ffi::String` | Shared-library stem used to locate the cdylib (e.g. `"cli_standard"`). Matches the crate cdylib filename |
| `command_name` | `safer_ffi::String` | Command namespace / top-level command name registered for this plugin (e.g. `"std"`) |
| `version` | `safer_ffi::String` | Plugin version, usually `env!("CARGO_PKG_VERSION")` |
| `author` | `safer_ffi::String` | Author, usually `option_env!("CARGO_PKG_AUTHORS")` |
| `description` | `safer_ffi::String` | Short one-line description |
| `homepage` | `safer_ffi::String` | Project homepage; empty string if none |
| `license` | `safer_ffi::String` | License identifier, usually `option_env!("CARGO_PKG_LICENSE")` |
| `min_host_version` | `TaggedOption<String>` | Minimum compatible host version; `None` = no lower bound |
| `max_host_version` | `TaggedOption<String>` | Maximum compatible host version; `None` = no upper bound |
| `pb_len` | `i32` | Reserved progress-bar length; `0` in the standard plugin |

Version compatibility is checked by the core library with a simple dotted numeric comparison; a mismatch returns a `PluginResult` with a nonzero exit code before `on_init_plugin` runs.

## Return types

- `Result` (core result): `{ code: i32, message: String }`. `code == 0` is success; use `Result::success()` / `Result::error(msg)`.
- `PluginResult` / `SrvPluginResult`: `{ success: i32, exit_code: i32, msg: TaggedOption<String> }`. `success` is a truth flag, `exit_code` a nonzero error code, `msg` an optional detail.

## Naming conventions

- **Crate / dylib**: the crate name uses kebab-case (`cli-standard`); the dylib stem (`dylib_name`) uses snake_case (`cli_standard`). The built cdylib matches the system naming convention (`libcli_standard.so` on Linux, `cli_standard.dll` on Windows, `cli_standard.dylib` on macOS).
- **Namespace**: `command_name` is the top-level command the plugin contributes. Subcommands and args are described in `command_yaml`.
- **Channel name** (server): a short snake_case or kebab_case routing key (e.g. `"send_message"`); it must match the `Packet.channel` the client sends.

## FFI data rules

- Every struct crossing the boundary is `#[derive_ReprC] #[repr(C)]`.
- Strings cross as `safer_ffi::String`; lists as `Out<'_, safer_ffi::vec::Vec<safer_ffi::String>>`.
- Business hook arguments are references: `&safer_ffi::String`, `&i32`; output parameters are `Out<'_, T>`.
- `TaggedOption::Some(v)` / `TaggedOption::None` is the optional type across FFI; do not pass Rust references or boxes across the boundary.

## Minimal skeleton

A client plugin's `lib.rs` follows this shape (full tutorial in [guide.md](guide.md)):

```rust
use cli_core_types::{HostMetadata, PluginMetadata, PluginResult};
use safer_ffi::{ffi_export, option::TaggedOption, prelude::*};
use std::sync::{LazyLock, OnceLock};

static METADATA: LazyLock<PluginMetadata> = LazyLock::new(|| PluginMetadata {
    command_yaml: include_str!("../commands.yaml").into(),
    name: "my-plugin".into(),
    dylib_name: "my_plugin".into(),
    command_name: "my".into(),
    version: env!("CARGO_PKG_VERSION").into(),
    author: option_env!("CARGO_PKG_AUTHORS").unwrap_or("").into(),
    description: "my first plugin.".into(),
    homepage: "".into(),
    license: option_env!("CARGO_PKG_LICENSE").unwrap_or("UNLICENSED").into(),
    min_host_version: TaggedOption::Some("0.1.0".into()),
    max_host_version: TaggedOption::None,
    pb_len: 0,
});

static LIB: OnceLock<libloading::Library> = OnceLock::new();

#[ffi_export]
pub fn get_plugin_metadata(out: Out<'_, PluginMetadata>) -> cli_core_types::Result {
    out.write(METADATA.clone());
    cli_core_types::Result::success()
}

#[ffi_export]
pub fn on_init_plugin(h_meta: HostMetadata) -> PluginResult {
    // Open the core library and resolve hooks here; register options/UI.
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}
```
