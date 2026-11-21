# Writing a Minimal Client Plugin

This tutorial scaffolds a small client plugin from scratch, loads it locally, and verifies it runs. It mirrors the structure of `crates/plugins/cli-standard`.

## 1. Create the crate

Add a new cdylib crate under `crates/plugins/`, for example `crates/plugins/my-plugin`.

`Cargo.toml`:

```toml
[package]
name = "my-plugin"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
version.workspace = true
authors.workspace = true
repository.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
clap = { workspace = true }
anyhow = { workspace = true }
prost = { workspace = true }
base64 = { workspace = true }
protos = { workspace = true }
safer-ffi = { workspace = true }
cli-core-types = { workspace = true }
cli-core-macros = { workspace = true }
libloading = { workspace = true }

[build-dependencies]
yaml2cmd-build = { workspace = true }

[lints]
workspace = true
```

Register the crate in the root `Cargo.toml` `[workspace] members` list.

## 2. Describe the command tree in YAML

Create `crates/plugins/my-plugin/commands.yaml`. A top-level entry becomes the command namespace; `args` are positional/named parameters, `subcommands` nest under a command:

```yaml
commands:
  - name: my
    about: My plugin commands
    subcommands:
      - name: hello
        about: Greet a user
        args:
          - name: who
            required: true
            value_name: WHO
            help: Who to greet
```

## 3. Build script and command skeleton

`build.rs` converts the YAML into the clap `Command` at build time:

```rust
use std::fs;
use std::path::Path;

fn main() {
    let yaml_path = Path::new("./commands.yaml");
    let yaml = fs::read_to_string(yaml_path).expect("Cannot read ./commands.yaml, file missing");
    yaml2cmd_build::build_with_yaml(&yaml);
    println!("cargo:rerun-if-changed={}", yaml_path.display());
}
```

Generate the per-command Rust skeleton with `cmd-gen` (run from the repository root):

```sh
cargo run -p cmd-gen -- plugins/my-plugin
```

This creates `src/commands/` with one module per command and a `mod.rs` implementing the `CommandExecutor` trait. Generated files carry the `NOT-MODIFIED-TAG` header. **If you hand-edit a generated file, delete that header tag** so it is not overwritten on the next regeneration. New command directories follow this generated layout; do not hand-write an aggregated `mod.rs`.

## 4. Plugin entry point

`src/lib.rs` implements the FFI contract from [plugin-contract.md](plugin-contract.md). It holds metadata and the loaded core library, exports the entry functions, and dispatches commands through the clap `Command` built from the YAML:

```rust
use anyhow::Result;
use cli_core_types::{HostMetadata, PluginMetadata, PluginResult, Result as CmdResult};
use cli_core_types::option::TaggedOption;
use cli_core_macros::load_core_symbols;
use libloading::Library;
use safer_ffi::ffi_export;
use safer_ffi::prelude::*;
use std::sync::{LazyLock, OnceLock};

mod commands {
    include!(concat!(env!("OUT_DIR"), "/command.rs"));
    pub static CMD: LazyLock<clap::Command> = LazyLock::new(|| {
        add_commands(
            clap::Command::new("my")
                .version(env!("CARGO_PKG_VERSION"))
                .about("My plugin commands"),
        )
    });
}

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

static LIB: OnceLock<Library> = OnceLock::new();

fn init_library(path: &str) -> &'static Library {
    LIB.get_or_init(|| unsafe { Library::new(path).expect("Failed to load library") })
}

#[ffi_export]
pub fn get_plugin_metadata(out: Out<'_, PluginMetadata>) -> CmdResult {
    out.write(METADATA.clone());
    CmdResult::success()
}

#[ffi_export]
pub fn on_init_plugin(h_meta: HostMetadata) -> PluginResult {
    let lib = init_library(&h_meta.cli_core_path);
    // Resolve core hooks and register options / UI here.
    load_core_symbols!(lib, register_option);
    let _ = register_option(&safer_ffi::String::from("MY_OPTION"));
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}

#[ffi_export]
pub fn run_command(args: &safer_ffi::Vec<safer_ffi::String>) -> CmdResult {
    let refs: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    match commands::CMD.clone().try_get_matches_from(refs) {
        Ok(matches) => match commands::dispatch(&matches) {
            Ok(()) => CmdResult::success(),
            Err(e) => CmdResult::error(&e.to_string()),
        },
        Err(err) => CmdResult::error(&err.to_string()),
    }
}

#[ffi_export]
pub fn on_shutdown_plugin() -> PluginResult {
    PluginResult { success: true.into(), exit_code: 0, msg: TaggedOption::None }
}
```

The `commands::dispatch` function and per-command executors are produced by `cmd-gen`; fill in each command's `execute(...)` body there.

## 5. Register UI and options

During `on_init_plugin`, resolve the hooks you need and register them. Examples (full signatures in [hooks.md](hooks.md)):

```rust
load_core_symbols!(lib, register_option, register_menu_item);

let _ = register_option(&"MY_OPTION".into());

let _ = register_menu_item(
    &"Say hello".into(),
    &"My".into(),
    &"M".into(),
    &cli_core_types::ui::ui_action_kind::RUN,
    &"my hello world".into(),
    &"".into(),
);
```

Menus, forms, sidebar sections/items, and status-bar fragments are all registered the same way; the host TUI renders from these registries only.

## 6. Build and load locally

```sh
# Check the new crate
cargo check -p my-plugin --locked

# Build the workspace; the cdylib lands in target/
cargo build --locked
```

Start the client and load the plugin by name:

```
tc> plugin load my-plugin
tc> my hello world
```

`plugin list` should show `my-plugin`; `namespace list` shows the `my` namespace. If the library cannot be found, verify `dylib_name` matches the built cdylib stem and that the host's library search path includes the `target` directory.

## Server plugins differ

A server plugin does not implement `run_command`. In `on_init_plugin` it opens `srv-core`, registers a channel per RPC, and binds a `ChannelHandlerFn`. See `crates/plugins/srv-standard/src/lib.rs` and the [server hooks section](hooks.md#server-hooks). Handlers decode the base64/prost request from `Packet.payload`, process it against plugin-owned state, and return a `PacketResult` with the encoded response.

## Next steps

- Re-read the [FFI contract](plugin-contract.md) for metadata fields and return codes.
- Look up specific hooks in [hooks.md](hooks.md).
- Study `cli-standard` for a complete reference: command dispatch, forms, sidebar, status bar, remote RPC calls, and completion sources.
