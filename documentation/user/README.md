# User Guide

This guide is for everyday users. After reading it you should be able to connect to a server, join a frequency to chat, send and receive files, and manage installed plugins. See [commands.md](commands.md) for the full command parameters, and [shortcuts.md](shortcuts.md) for interface keys.

## Basic concepts

- **Client**: the program running on your machine. It has two interfaces — a command-line **REPL** in the terminal and a full-screen **TUI**. They share the same commands; only the interaction style differs.
- **Server**: the backend program that relays messages and maintains frequencies and online users; it listens on port `50051` by default.
- **Frequency / freq**: a chat and call channel, similar to a chat room. After joining one you can see the members in that frequency and speak. A frequency can have a password, a member cap, and moderators.
- **User identity**: your full identity in the system is `username@IP` (for example `hfxd@127.0.0.1`). Commands that take a user argument (kick, mute, private message, etc.) usually expect this full identity.
- **Session**: a connection to the server. You can park the current session in the background (`background`) and open a new one, then switch between them with `sessions`.
- **Plugin** and **namespace**: commands and features are organized by plugin; each plugin exposes one namespace, its `command_name`. `plugin list` shows loaded plugins; `namespace list` shows loaded namespaces. You normally type a command directly (e.g. `connect`); core resolves it to the owning plugin through the loaded global command table. A namespace prefix (e.g. `std connect`) explicitly selects that plugin.
- **Option**: a behavioral setting. Options split into **global options** (`setg`/`getg`, kept across sessions) and **session options** (`set`/`get`, valid only for the current connection). Common built-in options are listed below.
- **Workspace**: an isolated set of aliases, macros, and local configuration. Use the `workspace` command to create, switch, import, and export one.

### Common built-in options

| Option | Meaning |
| --- | --- |
| `SERVER` | Server address, e.g. `127.0.0.1` |
| `PORT` | Server port, e.g. `50051` |
| `USERNAME` | Login nickname |
| `QUIET` | Set to `true` to silence background pushes and reduce screen noise |
| `PREFER_COMMAND_LINE` | Set to `true` so parameterized commands go straight to the terminal input instead of opening a parameter form |
| `EDITOR` | External editor invoked when composing long messages/macros |

## Choosing an interface

- **REPL**: type commands directly in the terminal command line. Suited to scripting, keyboard-driven flow, and background operation.
- **TUI**: a full-screen interface with a top menu, the chat log in the middle, a members/sessions/plugins sidebar on the right, and a status bar at the bottom. You can click menus with the mouse or drive it from the keyboard.

Launch without arguments for the REPL; pass `--tui` for the TUI.

## Five-step walkthrough

### 1. Configure the connection

```
tc> setg SERVER 127.0.0.1
tc> setg PORT 50051
tc> setg USERNAME hfxd
```

`setg` writes a global option that persists across restarts. You can also pass the address directly after `connect`: `connect 127.0.0.1:50051`.

### 2. Connect

```
tc> connect
Connect: success welcome, hfxd@127.0.0.1.
```

Use `disconnect` to drop the current connection, `reconnect` to re-establish it after a drop, and `status` to test reachability.

### 3. Join or create a frequency

```
tc> join_freq general
tc> create_freq dev --max-people 20
```

- `join_freq <frequency>`: join an existing frequency; add the password argument if it has one.
- `create_freq <frequency>`: create a frequency. `-p/--password` sets a password interactively, `--max-people` sets the member cap, `-n/--no-mod` creates it without a moderator.
- `leave_freq` leaves the current frequency; `freq_info` shows frequency details.

### 4. Send messages

```
tc> msg hello everyone
tc> reply agreed
tc> call alice@192.168.1.10
```

- `msg [message] [users...]`: send a message. Without users it goes to everyone in the current frequency; without a message body it opens the editor.
- `reply`: quickly reply to the last person who spoke to you.
- `call <user>`: call a user.
- Scheduled speech: `schedule_msg +5m remind everyone about the meeting later`; scheduled command: `schedule +10s status`; use `tasks` to list scheduled tasks and `cancel <task-id>` to cancel one.

### 5. Transfer files

```
tc> upload ./notes.txt /notes.txt
tc> download /notes.txt ./
tc> ls
tc> tree
```

Remote file operations (`cd`/`pwd`/`ls`/`mkdir`/`rm`/`mv`/`cp`, etc.) act on the server's current directory; local directories use `lls`/`lcd`/`lpwd`.

## Managing plugins

```
tc> plugin list              # loaded plugins
tc> plugin load <plugin>
tc> plugin unload <plugin>
tc> tcpm install <plugin>    # install from the registry
tc> tcpm search <keyword>
tc> tcpm list installed
```

`tcpm` (Terminal Chat Package Manager) handles plugin search, install, download, update, and uninstall. See the [plugins and package manager section](commands.md#plugins-and-package-management) in commands.md.

## Getting help

- Type `help` inside the command line; in the TUI, open the help popup from the top menu.
- List available option keys: `show options`; inspect current variable values: `show variables`.
- List online members, moderators, and frequencies: `show active_users`, `show moderators`, `show active_freqs`.
