# Architecture

This document explains how the pieces fit together: layering, the FFI boundary, dynamic loading, the wire protocol, the client event loop and state snapshots, server-side routing, and how hook types are generated.

## 1. Layering

```
┌──────────────────────────── cli host (binary) ────────────────────────────┐
│  REPL (rustyline) / TUI (edit-tui)  ·  input loop  ·  command dispatch       │
└─────────────── loads ────────────────────────────────┬────────────────────┘
                                                          │ libloading
┌──────────────────────── cli-core (cdylib) ─────────────▼───────────────────┐
│  registries (plugins / options / menus / forms / sidebar / bottom)          │
│  state (options, freq, aliases, macros) · push queue · terminal hooks       │
└─────────────── calls hooks ───────────────────────────┬────────────────────┘
                                                          │ C ABI
┌──────────────────────── client plugins (cdylib) ───────▼───────────────────┐
│  cli-standard · ai-glm · translations · werewolf …                          │
└───────────────────────────────────────────────────────────────────────────┘
```

The server side mirrors this with `srv` host, `srv-core` cdylib, and server plugins.

- **Host** owns the terminal or socket lifecycle, parses raw input, and dispatches commands/packets. It knows nothing about any plugin's concrete menu labels, sidebar sections, or status-bar text.
- **Core** owns the registries and shared state, exposes registration primitives and query hooks, and routes hooks through the Before/On/After chain.
- **Plugin** contributes commands, UI registration, and business rules. It calls core hooks during `on_init_plugin` and answers later through `run_command` (cli) or a channel handler (srv).

## 2. Dynamic loading and handle lifetimes

- The host locates its core library with `which_dylib` and opens it once through `libloading`. The core handle is held in a `OnceLock<Library>` for the process lifetime.
- A plugin cdylib is located by `dylib_name`, opened, and its `get_plugin_metadata` / `on_init_plugin` symbols are resolved. The host passes the core library path inside `HostMetadata`; the plugin opens the same core and resolves its own hook symbols.
- The plugin's `Arc<Library>` handle is stored in `PLUGIN_LIBS` and kept for the process lifetime. This matters on Windows, where each `LoadLibrary` returns a fresh handle; reloading by path would re-run the plugin's `static` initializers and lose in-process state. The same stored handle is reused for every later `run_command`.
- Libraries are not `dlclose`d while executing. After a registry entry is removed the command stops working; the handle is released when the process exits.
- Version compatibility: `min_host_version` / `max_host_version` are checked with dotted numeric comparison before `on_init_plugin`; a mismatch returns a nonzero exit code.

## 3. FFI boundary

Cross-boundary data follows `safer-ffi` rules:

- Structures are `#[derive_ReprC] #[repr(C)]`. Under the current `safer-ffi` version a ReprC struct **cannot embed a `Vec`** (`String` is opaque). Complex lists travel as flat record lines through `Out<'_, Vec<String>>`.
- Strings are `safer_ffi::String`; numbers are passed by reference (`&i32`, `&u64`); output parameters are `Out<'_, T>` written by the callee.
- Optionals use `TaggedOption::Some(v)` / `TaggedOption::None`; Rust references, boxes, or `Vec`s never cross the boundary directly.
- The core exposes `Result { code: i32, message: String }`; plugins return `PluginResult { success: i32, exit_code: i32, msg: TaggedOption<String> }`.

## 4. Hook type generation

`cli-core-types/build.rs` parses every `cli-core` source file and scans functions annotated with `#[register_hook]`. For each one it generates, into `OUT_DIR/fn_type.rs`:

1. The direct function-pointer type, e.g. `GetGlobalOptionFn = extern "C" fn(&String, Out<TaggedOption<String>>) -> Result`.
2. Before/On/After hook-chain types (`Before<Name>FnHook`, `On<Name>FnHook`, `After<Name>FnHook`) and their register functions, plus a `Clear<Name>HooksFn` clear hook.
3. The registration function types.

`cli-core-types/lib.rs` `include!`s that generated file. On the plugin side, `cli-core-macros`'s `load_core_symbols!(lib, register_option, …)` resolves each snake_case symbol and binds it to a local variable whose type is the matching `PascalCaseFn` from `cli_core_types`.

## 5. Wire protocol

No gRPC/tonic. The protocol is a custom frame with prost-encoded payloads:

- **Frame**: `u32` big-endian length prefix followed by the prost message bytes.
- **Handshake**: client sends `ConnectRequest`; server replies `ConnectResponse`.
- **Data**: both sides exchange `models.packet.Packet`:

  ```
  Packet { channel: String, sender: String, payload: String, timestamp: u64, seq: u64 }
  ```

  The business message is prost-encoded and base64-encoded into `payload`. The server normalizes `sender` to `user@ip`.
- **Routing**: `srv-core`'s `on_recieve_packet` reads `packet.channel`, looks up the registered `ChannelHandlerFn`, and calls it with the borrowed packet. The handler decodes the request, processes it, and returns a `PacketResult { handled, code, message, payload }` with the base64-encoded response.
- **Disconnect**: client sends `DisconnectRequest`; server replies and closes the session.
- Message schemas live in `crates/protos/proto/**/*.proto`; `protos/build.rs` compiles them with a vendored protoc so CI needs no system install.

## 6. Client event loop and state

### Background command worker

Commands run off the UI thread in a single serial `CommandWorker`:

- A `tx/rx` channel carries `(argv, silent)` jobs; an `AtomicBool` tracks running state; a `Mutex<Option<JobResult>>` holds the completed result.
- The per-job silent marker is an `Arc<AtomicBool>` set on `submit` and kept across completion until the next submit overwrites it. This binds echo attribution to the job whose drained lines are still in the buffer, instead of a shared global bool that a neighboring job's completion callback could flip early.
- Real-time output is drained per frame: non-silent jobs echo to the output log; silent backfill only accumulates into `worker_output` for sidebar reconcile and never enters the log.

### Push queue

Asynchronous pushes from the server are enqueued by the plugin's background reader thread through `tc_cli_push_enqueue`. The host receive thread blocks on `tc_cli_push_wait` (Condvar, woken on enqueue, with a timeout heartbeat), then drains with `tc_cli_push_drain`. The queue lives in `cli-core` rather than the plugin because a plugin can be loaded more than once; its own statics would not be shared across instances, while the core is loaded once and reached through the host handle. `QUIET` silences drained pushes.

### Sidebar snapshots and UI registry

- The host holds owned snapshots: `members`, `sessions`, `plugins`. Backfill refreshes (`show active_users`, `sessions -l`, `plugin list`) run as silent background jobs and update only these snapshots.
- Reconciliation normalizes each snapshot with a stable sort and name-based dedupe: members by string, plugins by name, sessions by `(!current, name)` (current session first). Snapshots are replaced only when the normalized list differs (`PartialEq` guard), so order jitter does not repaint the whole section.
- UI is registry-driven: the host fetches flat record rows from core (`Out<Vec<String>>`, fields `|`-separated) for menu items, form specs, sidebar sections/items, and bottom fragments, parses them into owned models, and renders.
- Menu groups and sidebar section/item templates are cached in the host. After any plugin/namespace load or unload command (whether it reports success or not), the idle event loop re-fetches the menu and sidebar registries, rebuilds the menu groups, and re-fills the sidebar templates (bottom fragments are re-fetched every frame). The runtime roster/session/plugin-name snapshots are a separate silent-backfill path; a zero-match namespace unload still triggers the refetch so stale entries do not linger.
- Each registered entry records its owning plugin through an internal core owner context set during `on_init_plugin`; the FFI registration primitives take no owner parameter. `unregister_plugin` invokes `on_shutdown_plugin`, then drops the plugin's metadata/namespace/global-command entries and every option/UI entry owned by it. The dylib handle is retained for the process lifetime to avoid executing from an unloaded code page; persisted aliases are user configuration and are not touched on unload.
- Command routing keeps two maps: namespace (`command_name`) → plugin, and global command name → plugin. The global map is populated at load from the top-level command names parsed out of the plugin `command_yaml`. Dispatch matches an explicit namespace first, then the global map; core supplies the namespace token internally when calling the plugin, instead of rewriting the user command line with a hard-coded default namespace.

### Completion candidates

Completion is driven by a unified command table in `cli-core` (`registered_commands.rs`); the host keeps no hard-coded command or subcommand list:

- **Command names and subcommands**: built-in commands are registered in the table at initialization; plugin commands, subcommands, and argument specs are parsed from each plugin's `command_yaml` at load. The host reads top-level names through `list_commands` and a parent's direct subcommands through `list_subcommands`. `list_commands` also appends every loaded plugin's namespace (`command_name`), since the router accepts a namespace as the first token; names are de-duplicated.
- **Argument specs**: each argument's declared `type` is mapped to a `ui_field_kind` category. `arg_spec(command_path, arg_index)` returns one flat row `kind|custom`. The incoming path may already contain filled arguments, so core locates the entry by longest registered-prefix match and recomputes the argument index against the matched command's depth. Options (a flag or valued option declared with `short`/`long` or an `action`) do not occupy a positional slot; the positional index counts only positional arguments, so a flag declared before a positional argument does not shift that argument's spec.
- **Built-in categories** reuse existing host snapshots and registries: member names from the sidebar member snapshot, plugin names and namespaces from the live plugin registry (`get_all_plugins`), option keys from the registered-option registry (`get_registered_options`), remote paths from the plugin's directory listing, and local paths from the local filesystem. The REPL pulls plugin/option candidates live per completion (with the startup snapshot only as a fallback when the symbol is missing), so unloading a plugin removes its names, namespace, and option keys from the next completion; the first-word list comes from the live core command table rather than the startup clap command tree.
- **Custom concepts**: a type name core does not recognize is kept as the raw `custom` type name. The host routes that argument position to `complete_arg`, which resolves the owning plugin by the first token and forwards to its optional `tc_complete_arg` callback; a missing owner, symbol, or empty result yields no candidates.
- A command or position that cannot supply candidates falls back to `NO_COMPLETION`; free text maps to `PLAIN`.

## 7. Server side

- **Sessions**: `srv` keeps a per-connection session and maps the peer address to the handshake username; `srv-core` exposes add/remove/count session hooks.
- **Channels**: a plugin registers `ChannelMetadata { name, channel_type, description, version, author, capacity }`, then binds a handler to the channel name. Channel types: `CHAT=0`, `CALL=1`, `FILE=2`, `BROADCAST=3`, `CUSTOM=4`.
- **State**: business state (frequency rooms, user profiles, banned sets, per-user virtual filesystem, pending notifications) lives inside the server plugin behind its own mutexes; the core is agnostic to it.
- **Out-of-band push**: a plugin resolves `tc_srv_pusher_push(user, bytes, len)` to push an encoded `Packet` frame to an online user outside the request/response cycle. Offline targets are skipped silently; the notification stays queued for later poll.

## 8. Concurrency and state ownership

- Core registries and shared state use `LazyLock<Mutex<…>>` (or atomics) and are mutated only through core hooks.
- Plugin-internal state is guarded by its own locks; plugins must not hold a lock across a core call that could re-enter.
- The UI thread renders from snapshots; background workers and reader threads never mutate UI nodes directly.

## 9. Internationalization (i18n)

- Dictionaries live at the top level `i18n/cli.toml` and `i18n/srv.toml`. Only two languages are retained: `en` (English, required) and `zh` (Simplified Chinese). Each entry is a `[key]` section with `en = "…"` and `zh = "…"`; placeholders such as `{user}` / `{ip}` are left for the caller to fill with `replace` / `format!`.
- `crates/i18n-build` is a std-only build-dependency. Each host's `build.rs` reads its dictionary, calls `i18n_build::generate`, and writes a Rust module into `OUT_DIR`; the host `include!`s it under an `i18n` submodule. The generated module exposes a `Lang` enum, `detect_lang()`, a static lookup `TABLE`, `t(key)` for the active language, and `t_lang(lang, key)` used by tests.
- Language detection order: `TC_LANGUAGE` wins, then `LANG`, then `LC_ALL`. A value starting with `zh` maps to Chinese; anything else, and the empty/missing case, falls back to English. The detected result is cached in a `OnceLock`.
- Lookup rules: an unknown key echoes the key itself; a missing `zh` entry falls back to `en`. The parser rejects a missing `en`, duplicate keys, and any language other than `en`/`zh`.
- First-batch scope: built-in `cli` host UI strings (menus, dialogs, REPL/startup messages) and the two core `srv` handshake messages. Plugin menu labels, form fields, and command output in `cli-standard` / `srv-standard` still ship fixed English text and can be migrated in a later batch through the same mechanism.
