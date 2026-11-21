# Interface & Shortcuts

## TUI layout

After entering the TUI (`cargo run -p cli -- --tui`), the interface is split into:

- **Top menu bar**: menu items registered by plugins, grouped together. Click with the mouse, or expand a group with a keyboard shortcut.
- **Chat log**: shows results of commands you actively run, plus messages and call notifications pushed by the server. Background auto-refresh of member/session/plugin lists only updates the sidebar on the right; it does not flood this area.
- **Right sidebar**: collapsible sections such as `Sessions`, `Members`, and `Plugins`. Click a section title to collapse/expand it.
- **Bottom status bar**: shows the current user, frequency, quiet state, time, and other fragments (registered by plugins).
- **Input box**: at the bottom; type a command and press Enter to run it.

## Keyboard shortcuts (TUI)

| Key | Action |
| --- | --- |
| `Enter` | Run the command in the input box; accept the highlighted candidate when the completion popup is active; confirm in a popup |
| `Tab` | Accept the highlighted completion candidate and fill it into the input box |
| `↑` / `↓` | Recall command history; move between candidates when the completion popup is active |
| `Esc` | Close the current popup / overlay |
| `F10` | Open the top menu |
| `Alt+mnemonic` | Open the matching menu (the underlined letter marked on the menu item) |
| `Ctrl+C` | Interrupt a running background command; does not exit when no command is running |
| `Ctrl+Q` | Exit the TUI |
| `PgUp` | Scroll the chat log up one page |

## Mouse operations (TUI)

- **Menu**: click a menu item to trigger its action directly (run the command line or open the parameter form).
- **Sidebar entry**: double-click (or press `Enter` on the entry) to append its name to the end of the main input box, separated by a space.
- **Sidebar title**: click to collapse / expand the section.
- **Popup button**: click to activate; with focus on a button, `Enter` confirms, `Esc` cancels.

## Form popups

Parameterized commands open a parameter form when you are not in command-line preference mode:

- Fill in the fields one by one; fields marked secret (password) do not echo input.
- `Enter` confirms and submits, `Esc` cancels.
- To run such commands directly on the terminal command line instead, set `setg PREFER_COMMAND_LINE true`.

## REPL interface

Launch without `--tui` for the REPL command line:

- `↑` / `↓`: recall command history.
- `Tab`: trigger completion for commands, subcommands, and arguments (member names, plugin names, namespaces, remote paths, etc.; candidates come from the current sidebar snapshot and registered options).
- Press Enter after typing a command to run it; when a command runs in the background its output syncs live without blocking input.

## Quick reference

- Open help: the Help group in the menu, or `help` on the command line.
- List online members: `show active_users`; list frequencies: `show active_freqs`.
- Interrupt a stuck background command: `Ctrl+C`.
- Exit: `Ctrl+Q` in the TUI; exit the shell to leave the REPL.
