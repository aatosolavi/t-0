# Browser Terminal Notes

This branch runs a real local shell inside a browser tab:

- HTML server: `http://localhost:4321`
- PTY broker: `ws://localhost:4322`
- Entry command: `bun run terminal`

The PTY broker is a Node process because `@lydell/node-pty` is more reliable there than under Bun on macOS. The Bun process only serves the HTML and attachment upload endpoint.

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

Canonical repo path: `~/dev/mission-control` (not iCloud Documents). The install script records `WorkingDirectory` and `ProgramArguments` from the checkout you run it in.

```bash
cd ~/dev/mission-control
./terminal/install-launch-agent.sh
# or: bun run terminal:install  (also rebuilds the `mc` launcher)
```

It installs `~/Library/LaunchAgents/com.grok-mission-control.terminal.plist` and starts the terminal stack at login. Logs go to:

- `~/.grok-mission-control/logs/terminal.out.log`
- `~/.grok-mission-control/logs/terminal.err.log`

Manual control:

```bash
launchctl kickstart -k gui/$(id -u)/com.grok-mission-control.terminal
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.grok-mission-control.terminal.plist
```

## Launcher Direction

The browser page should stay a terminal surface. Workspace/app selection is moving into a native TUI that runs inside the PTY:

- Ratatui launcher crate: `terminal/launcher-ratatui`
- Install command: `bun run terminal:launcher:install`
- Installed command: `~/.grok-mission-control/bin/mc`
- Dev fallback path: `terminal/launcher-ratatui/target/release/mc`
- The PTY broker automatically starts the installed binary when it exists.
- If the binary is missing, the broker falls back to the normal login shell.
- Set `GROK_TERMINAL_USE_LAUNCHER=0` to force shell-first behavior.

The TUI scans `~/dev`, shows repos centered in the terminal, and supports keyboard and mouse input through terminal events. App choices: **Grok**, **Codex**, **Claude**, **Amp**, **Devin**, **Droid**, and **Shell** (keys `1`–`7`, or Tab).

Workspace selection is ordered by recent use. Each launch writes the selected cwd to:

```text
~/.grok-mission-control/recent-workspaces.txt
```

Typing normal characters filters workspace names and paths live. `Backspace` edits the filter, and `Esc` clears the filter before closing the launcher. From a shell, run `mc` to open Mission Control again.

`?cwd=/absolute/path` still bypasses the launcher and starts a shell directly in that path. That keeps direct deep links useful.

Each browser tab gets a generated session id in `sessionStorage`, so reloading the page reattaches to the same PTY session and replays recent terminal output. A new tab gets a new session. Disconnected tabs are retained for 6 hours by default so laptop sleep or temporary network/browser disconnects do not immediately kill running sessions. Override with `GROK_TERMINAL_SESSION_RETAIN_MS`.

Stable named local addresses are supported:

```text
http://localhost:4321/t/main
http://localhost:4321/?session=main
```

Those attach to the same named local session from any tab.

Bun is only the script runner and HTML server here. The launcher is a native Rust binary built by Cargo and installed into the app-owned bin directory.
