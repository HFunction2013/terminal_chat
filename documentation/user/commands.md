# Command Reference

Commands are grouped by usage scenario. Parameter names match the built-in help; `<UPPERCASE>` is required, `[brackets]` are optional. Aliases are given in parentheses after the command. Full parameters can be inspected in-program with `help <command>`.

## Connection & sessions

| Command | Purpose | Main parameters / examples |
| --- | --- | --- |
| `connect` (`conn`) | Connect to the server | `connect [HOST[:PORT]]`, e.g. `connect 127.0.0.1:50051` |
| `disconnect` (`dc`) | Drop the current connection | — |
| `reconnect` (`rc`) | Reconnect to the server | — |
| `restart` | Reload runtime state; `--hard` respawns the client process | `restart` reloads aliases/macros/options in-process; `restart --hard` exits and relaunches with the original arguments |
| `status` | Test network reachability | — |
| `whoami` | Show the current username | — |
| `away [STATUS]` | Set away/busy status; auto-reply to private messages | STATUS is only `away`/`busy`, default `away` |
| `sessions` | Multi-session manager | `sessions -l` list; `sessions -i <SESSION_ID>` switch; `sessions -k <SESSION_ID>` close |
| `background` (`bg`) | Park the current session in the background, return to the main console | — |
| `foreground` (`fg`) | Bring back to foreground when there is a single session | — |

## Frequencies & member management

| Command | Purpose | Main parameters / examples |
| --- | --- | --- |
| `join_freq` (`jfreq`) | Join a frequency | `join_freq <FREQ_ID> [PASSWORD]` |
| `create_freq` (`cfreq`) | Create a frequency | `create_freq <FREQ_ID> [PASSWORD]`; `-p/--password` sets password interactively; `--max-people <NUM>`; `-n/--no-mod` creates without a moderator |
| `leave_freq` (`lfreq`) | Leave the current frequency | — |
| `freq_info [FREQ]` | Show frequency information | Omit the argument to inspect the current frequency |
| `show active_freqs` (`afreqs`) | List all active frequencies | — |
| `show active_users` (`ausers`) | List online members in the current frequency | — |
| `show moderators` (`mods`) | List moderators of the current frequency | — |
| `op <USER>` | Grant moderator rights | USER is the full `name@ip` identity |
| `deop <USER>` | Revoke moderator rights (the creator cannot be removed) | — |
| `trop <USER>` | Transfer creator rights | — |
| `kick <USER>` | Kick a user from the frequency (moderator) | — |
| `ban <USER>` | Ban a user | — |
| `unban <USER>` | Lift a ban | — |
| `mute <USER>` | Mute a user | `set QUIET true` silences everyone except moderators |
| `unmute <USER>` | Unmute a user | — |
| `disband` | Disband the current frequency | `-f/--force` disbands forcibly with members present |

## Messages & calls

| Command | Purpose | Main parameters / examples |
| --- | --- | --- |
| `msg` | Send a message | `msg [-m message] [USERS...]`; without users, sends to the whole frequency; without a body, opens the editor |
| `reply` | Quickly reply to the last private message / call from you | `reply [-m message]` |
| `call <USER>` | Call a user | — |
| `ignore <USER>` | Ignore a user's messages | — |
| `unignore <USER>` | Stop ignoring a user | — |
| `user_info <USER>` | Show user information | — |
| `feedback [body]` | Send feedback; opens the editor without a body | — |
| `notifications` | Pull pending notifications (message/call/mute/kick/ban) | — |

## Scheduled tasks

| Command | Purpose |
| --- | --- |
| `schedule_msg <TIME> -m <message> [USERS...]` | Send a message on a schedule. TIME: `+30s`/`+5m`/`+2h`/`+1d`, local `HH:MM`, or a raw number of seconds |
| `schedule <TIME> <CMD...>` | Run a command on a schedule (argv form). TIME as above |
| `tasks` | List scheduled tasks |
| `cancel <TASK_ID>` | Cancel a scheduled task |

## Remote files (server directory)

| Command | Purpose | Parameters |
| --- | --- | --- |
| `pwd` | Print the server working directory | — |
| `cd <DIR>` | Change the server directory | — |
| `ls [DIR]` | List the server directory | — |
| `tree [DIR]` | Show the directory tree | — |
| `mkdir` (`md`) `<DIR>` | Create a directory | — |
| `upload` (`upl`) `<LOCAL_PATH> [REMOTE_PATH]` | Upload a file/directory | — |
| `download` (`dl`) `<REMOTE_PATH> [LOCAL_PATH]` | Download a file/directory | `-r/--recursive` download directories recursively |
| `rm` (`remove`) `<PATH>` | Delete a remote file/directory | `-r/--recursive` |
| `mv` (`move`) `<FROM> <TO>` | Move/rename | — |
| `cp` (`copy`) `<FROM> <TO>` | Copy | — |
| `file_info <FILE>` | Show remote file information | — |
| `checksum <FILE>` | Compute remote file SHA256 (directories unsupported) | — |

### Local directories (your machine)

| Command | Purpose |
| --- | --- |
| `lpwd` | Print the local working directory |
| `lcd [DIR]` | Change the local directory |
| `lls` | List the local current directory |

## Local commands & editor

| Command | Purpose | Parameters |
| --- | --- | --- |
| `shell` (`sh`) `<COMMAND...>` | Run a local shell command on this machine (not remote). stdout/stderr are captured and echoed to the output area by default; a `[finish]` line reports elapsed time and return value | `--tui` inherit the terminal to run an interactive TUI program |
| `editor [FILE]` | Open the editor | `-e/--editor <EDITOR>` choose the editor |
| `route` | Trace the route to the server | — |

## Configuration options

| Command | Purpose | Parameters |
| --- | --- | --- |
| `set <KEY> <VALUE>` | Set a session option | — |
| `get <KEY>` | Read a session option | — |
| `setg <KEY> [VALUE]` | Set a global option | `-p/--password` read the value as a password |
| `getg <KEY>` | Read a global option | — |
| `unset [KEY]` | Clear a session option | `-a/--all` clear all session options |
| `unsetg [KEY]` | Clear a global option | `-a/--all` clear all global options |
| `show options` | List available option keys | — |
| `show globals` | Show all global variables | — |
| `show locals` | Show all session variables | — |
| `show variables` (`vars`) | Show all variables | — |
| `show plugins` (`namespaces`) | List loaded plugins and their namespaces | — |
| `proxy on` / `proxy off` | Toggle the system proxy | — |

## Aliases & macros

| Command | Purpose |
| --- | --- |
| `create alias <CMD> <ALIAS>` | Create an alias |
| `create macro <MACRO_NAME> [-c <BODY>]` | Create a macro; without a body, open the editor to enter it |
| `run <MACRO_NAME>` | Run a macro |
| `delete alias <ALIAS>` | Delete an alias |
| `delete macro <MACRO_NAME>` | Delete a macro |

## Plugins and package management

### Plugin / namespace loading

| Command | Purpose |
| --- | --- |
| `plugin list` | List loaded plugins |
| `plugin load <PLUGIN>` | Load plugin code; the namespace stays unmounted, so its commands are not routable / completable until `namespace load` |
| `plugin unload <PLUGIN_NAME>` | Unload the plugin completely: run shutdown, remove its commands and all registered entries |
| `namespace list` | List mounted namespaces (`command_name`) |
| `namespace load <NAMESPACE>` | Mount a namespace: load its commands into the routing / command table; idempotent if already mounted |
| `namespace unload <NAMESPACE_NAME>` | Detach a namespace: remove routing entries, keep the plugin code loaded |
| `load plugin <PLUGIN>` | **Deprecated.** Use `plugin load`. Load a single plugin by name |
| `load namespace <NAMESPACE>` | **Deprecated.** Use `namespace load`. Resolve by `command_name`, not a text manifest group |
| `unload plugin <PLUGIN_NAME>` | **Deprecated.** Use `plugin unload`. Unload one plugin by name |
| `unload namespace <NAMESPACE_NAME>` | **Deprecated.** Use `namespace unload`. Resolve by `command_name` |

The sidebar lists loaded plugins. A mounted namespace is shown with a trailing `*` (e.g. `std*`); a loaded-but-unmounted plugin shows its namespace without `*`. Only `cli-standard` is mounted by default at startup; other plugins mount explicitly via `namespace load`.

### `tcpm` package manager

| Subcommand | Purpose |
| --- | --- |
| `tcpm install <PLUGIN...>` | Install plugins; `--registry <URL>` pick a source; `-u/--uload` load the namespace on startup; `-l/--local <PATH>` install locally; `-f/--force` overwrite an older version |
| `tcpm uninstall <PLUGIN...>` | Uninstall; `-d/--delete-local-file` also remove the local file |
| `tcpm download <PLUGIN...>` | Download; `-l/--load` load after download; `-u/--use-namespace` load the namespace together after download |
| `tcpm search <keyword>` | Search the registry |
| `tcpm show <PLUGIN>` | Show registry info; `-l/--local` view local storage |
| `tcpm list installed` / `downloaded` / `loaded` / `registry` / `updates` | List plugins filtered by constraint |
| `tcpm update <PLUGIN...>` | Update; `--registry <URL>`; `-r/--reload` reload after update; `-f/--force` overwrite; `standard` refers to the main program |

## Workspace

The `workspace` subcommands: `new`, `rename` (alias `mv`), `delete` (alias `del`), `switch` (alias `sw`/`use`), `list` (alias `ls`), `show` (alias `info`/`get`), `clone` (alias `cp`), `export`, `import`.

```
tc> workspace new work-project --enter
tc> workspace switch work-project
tc> workspace export -o ./work-project.toml
tc> workspace import ./work-project.toml
```

## Debug & hidden commands

- `logs [open|close]`: toggle logging (debug builds only).
- `sleep <TIME>`: hidden placeholder command.
