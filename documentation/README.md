# Documentation

This directory is organized by reader role. Identify which category you belong to and start from the matching entry.

## I am a user

You just want to use the tool: connect to a server, join a frequency to chat, transfer files, and manage installed plugins.

- [user/](user/README.md) — Basic concepts, interfaces, and getting started
- [user/commands.md](user/commands.md) — Command reference grouped by scenario
- [user/shortcuts.md](user/shortcuts.md) — Keyboard shortcuts and interface operations

## I want to develop plugins

You need to add commands, menus, sidebar entries, or status-bar fragments to the client, or write server-side channel handlers.

- [developer/](developer/README.md) — Plugin development overview and learning path
- [developer/plugin-contract.md](developer/plugin-contract.md) — Plugin FFI contract: metadata and entry functions
- [developer/hooks.md](developer/hooks.md) — Core library hook API reference
- [developer/guide.md](developer/guide.md) — Step-by-step tutorial for writing a minimal plugin

## I want to contribute to the project

You need to understand the layering and dynamic-loading mechanism between host, core library, and plugins, and change the framework itself.

- [contributor/](contributor/README.md) — Architecture overview and crate map
- [contributor/architecture.md](contributor/architecture.md) — Layering, FFI, event loop, and state snapshots
- [contributor/workflow.md](contributor/workflow.md) — Build/test/CI, code style, and contribution process

> Documentation can lag behind the code. If you find an inconsistency, trust the code and open an issue to correct the docs.
