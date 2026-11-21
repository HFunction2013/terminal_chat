# Terminal Chat

A terminal chat tool written in Rust. The client provides two interfaces — a REPL and a full-screen TUI — while the server relays messages, frequencies (channels), calls, and remote file operations over TCP. The whole system uses a layered **host + core library + plugin** architecture; commands, menus, forms, sidebar entries, and the status bar are all registered and driven by plugins.

- Language: Rust (edition 2024, rustc 1.93+)
- Protocol: custom frames (u32 big-endian length prefix + [prost](https://github.com/tokio-rs/prost) encoding); no gRPC/tonic dependency
- Plugin mechanism: `cdylib` dynamic libraries exposing a C ABI via `safer-ffi`/`hook_macro`, loaded by the host through `libloading`
- License: MIT

## Quick start

> Run the following commands from the repository root. The first build fetches and compiles every workspace crate.

### 1. Start the server

```sh
cargo run -p srv
# Listens on port 50051 by default; override with --port or the SRV_PORT env var.
cargo run -p srv -- --port 50051
```

### 2. Start the client

The REPL command line is the default interface; pass `--tui` for the full-screen TUI:

```sh
# REPL
cargo run -p cli

# Full-screen TUI
cargo run -p cli -- --tui
```

After startup, connect to the server from the client (address format `HOST[:PORT]`):

```
tc> setg SERVER 127.0.0.1
tc> setg PORT 50051
tc> setg USERNAME hfxd
tc> connect
```

### 3. Shell completion (optional)

```sh
cargo run -p cli -- completion bash    # bash / zsh / fish / powershell / elvish
```

## Build & test

```sh
# Full check (matches CI)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo check --workspace --all-targets --locked

# Tests
cargo test --workspace

# Single crate
cargo test -p cli
```

## Repository layout (overview)

| Directory | Responsibility |
| --- | --- |
| `crates/cli` | Client host binary: REPL / TUI dual interface; loads the core library and client plugins |
| `crates/srv` | Server host binary: TCP listener, session management, packet dispatch |
| `crates/cli-core` | Client core library: plugin/option/state/UI registries, command run, push queue (cdylib) |
| `crates/srv-core` | Server core library: channels, sessions, packet routing, plugin registration (cdylib) |
| `crates/cli-core-types` / `srv-core-types` | ReprC types and function-pointer definitions crossing the FFI boundary |
| `crates/cli-core-macros` | Procedural macro for plugin side to load core library symbols |
| `crates/plugins/cli-standard` | Standard client commands and default UI registration |
| `crates/plugins/srv-standard` | Standard server business: frequencies, users, messages, calls, remote files |
| `crates/plugins/` (`ai-glm`, `translations`, `werewolf`) | Optional / example plugins |
| `crates/edit-tui` | Built-in TUI framework (retained-mode component tree, framebuffer diff, layout, input) |
| `crates/protos` | prost-generated protocol messages |
| `crates/helper/cmd-gen` | Helper that generates command skeletons from `commands.yaml` |

For the full crate map and low-level implementation, see the [contributor documentation](documentation/contributor/README.md).

## Documentation

Pick the entry point that matches your role:

| Reader | Documentation | Goal |
| --- | --- | --- |
| **User** | [documentation/user](documentation/user/README.md) | Learn the basic concepts; connect, chat in a frequency, transfer files, manage plugins |
| **Plugin developer** | [documentation/developer](documentation/developer/README.md) | Master the plugin contract and core hook API; write client/server plugins independently |
| **Contributor** | [documentation/contributor](documentation/contributor/README.md) | Understand the architecture, layering, and key implementations; work on the host/core libraries |

## Contributing

Issues and pull requests are welcome. Before making changes, read the [contributor workflow](documentation/contributor/workflow.md), which covers the build/test/CI conventions, code style, and the `cmd-gen` command-skeleton rules.
