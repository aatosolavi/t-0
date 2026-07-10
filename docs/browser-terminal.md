# Browser Terminal Notes

**T-0** is a real local shell inside a browser tab:

- HTML server: `http://127.0.0.1:4321` (bind host configurable via `MC_BIND_HOST`)
- PTY broker: `ws://127.0.0.1:4322`
- Entry command: `bun run terminal`
- Accent: orange (`#f97316` / `#fb923c`)
- Themes: **system / light / dark** — `?theme=system|light|dark`, or **⌘/Ctrl+Shift+L** to cycle (stored in `localStorage`)
- Workspace root: `MC_WORKSPACE_ROOT` (default `~/dev` if present, else `$HOME`)
- Data dir: `MC_DATA_DIR` → `~/.mission-control` (legacy `~/.grok-mission-control` still works)

The PTY broker is a Node process because `@lydell/node-pty` is more reliable there than under Bun on macOS. The Bun process only serves the HTML and attachment upload endpoint. Both bind to **127.0.0.1** by default.

## Known Rendering Issue

Codex/Grok TUI input surfaces can show renderer-sensitive issues: bottom-row clipping, cursor/background mismatch with a blinking block cursor, and broken-looking box drawing.

Current understanding:

- Moving `.xterm-screen` with CSS can desync xterm's internal geometry from the actual rendered viewport and clip the final row.
- xterm renders the block cursor over the actual terminal buffer cell. When the cursor blinks off, xterm reveals the cell underneath.
- Some TUIs appear to paint the grey input surface around the cursor but leave the blank cursor cell itself with the default terminal background.
- In that case, the cursor cell looks like it changes background on blink-off, but the browser is showing the real underlying buffer state.
- The DOM renderer draws box characters through the browser font. xterm's `customGlyphs` option only helps on canvas/WebGL renderers.

Current browser-side mitigation:

- The browser imports `@xterm/xterm` and uses the DOM renderer by default.
- `.xterm-screen` is not translated; fitting is handled by `FitAddon.fit()`.
- `lineHeight` is slightly relaxed.
- Block cursor blinking is off by default to avoid blink-off background flicker. Use `?blink=1` to test blinking.
- Use `?renderer=canvas` to force the canvas renderer for comparison.

Preferred future fixes:

1. Best: fix the TUI output so the input surface paints every cell, including the cursor cell.
2. Keep the browser terminal identified as `TERM=xterm-256color`; do not advertise Ghostty-specific terminfo to xterm.js.
3. If the DOM renderer has browser-specific issues, use `?renderer=canvas` as a fallback and expect custom glyphs to be renderer-dependent.

## macOS Login Startup

The install script records `WorkingDirectory` and `ProgramArguments` from the checkout you run it in.

```bash
./terminal/install-launch-agent.sh
# or: bun run terminal:install  (also rebuilds the `mc` launcher)
```

It installs `~/Library/LaunchAgents/com.mission-control.terminal.plist` (and removes the legacy `com.grok-mission-control.terminal` agent if present). Logs go to `$MC_DATA_DIR/logs/` (usually `~/.mission-control/logs` or the legacy data dir).

Manual control:

```bash
launchctl kickstart -k gui/$(id -u)/com.mission-control.terminal
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.mission-control.terminal.plist
```

## Launcher Direction

The browser page should stay a terminal surface. Workspace/app selection is moving into a native TUI that runs inside the PTY:

- Ratatui launcher crate: `terminal/launcher-ratatui`
- Install command: `bun run terminal:launcher:install`
- Installed command: `$MC_DATA_DIR/bin/mc` (default `~/.mission-control/bin/mc`)
- Dev fallback path: `terminal/launcher-ratatui/target/release/mc`
- The PTY broker automatically starts the installed binary when it exists.
- If the binary is missing, the broker falls back to the normal login shell.
- Set `GROK_TERMINAL_USE_LAUNCHER=0` to force shell-first behavior.

The TUI scans `MC_WORKSPACE_ROOT` (default `~/dev` or `$HOME`), shows repos centered in the terminal, and supports keyboard and mouse input through terminal events. App choices: **Grok**, **Codex**, **Pi**, **Claude**, **Amp**, **Devin**, **Droid**, and **Shell** (keys `1`–`8`, or Tab). Missing CLIs are dimmed.

### Cold-start splash

On **process start** of `mc` (new tab / new PTY), a short **T-0** splash uses the same bordered panel + orange accent as the picker. It does **not** reappear when an agent exits back to the launcher. Any key skips. Disable: `MC_SPLASH=0`.

### Memory

Stored in `$MC_DATA_DIR/launcher-state.json`:

| Key | Behavior |
|---|---|
| (auto) | Last agent per workspace — re-selects when you highlight a repo |
| `space` | Toggle favorite (filter must be empty); favorites sort first with `★` |
| `.` | Continue last workspace + agent (filter must be empty) |

List order: **favorites → recents → last cwd → root scan**.

### Side actions (filter empty)

| Key | Action |
|---|---|
| `e` | Open workspace **folder in IDE** (`open -a Cursor` when Cursor.app exists; not the agent CLI) |
| `f` | Reveal in Finder (`open`) |
| `c` | Copy absolute path (`pbcopy`) |
| `g` | Open `origin` remote in the browser (GitHub-style URLs) |
| `s` | Settings — splash, default agent, default IDE for `e` |

App chip **Cursor** launches the **Cursor Agent** CLI (`agent` / `cursor-agent`). The shell command `cursor` on many installs is only a shim and does not open the IDE.

## Branding

User-facing name: **T-0** (launch countdown — liftoff is now).  
Repo / package / data dir / `mc` binary keep `mission-control` paths for continuity.

### Git row metadata

Each git workspace shows **branch**, **`*`** when dirty, and **`↑N`** commits ahead of upstream when available. Remembered agent name is shown on the row in the accent color.

Recents still also write:

```text
$MC_DATA_DIR/recent-workspaces.txt
```

Typing normal characters filters workspace names and paths live. `Backspace` edits the filter, and `Esc` clears the filter before closing the launcher. From a shell, run `mc` to open T-0 again.

`?cwd=/absolute/path` still bypasses the launcher and starts a shell directly in that path. That keeps direct deep links useful.

Each browser tab gets a generated session id in `sessionStorage`, so reloading the page reattaches to the same PTY session and replays recent terminal output. A new tab gets a new session. Disconnected tabs are retained for 6 hours by default so laptop sleep or temporary network/browser disconnects do not immediately kill running sessions. Override with `GROK_TERMINAL_SESSION_RETAIN_MS`.

Stable named local addresses are supported:

```text
http://localhost:4321/t/main
http://localhost:4321/?session=main
```

Those attach to the same named local session from any tab.

Bun is only the script runner and HTML server here. The launcher is a native Rust binary built by Cargo and installed into the app-owned bin directory.
