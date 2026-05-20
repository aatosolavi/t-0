# Mission Control Ratatui Launcher

This is the native startup screen for the browser terminal.

It runs inside the PTY, so xterm.js only has to be a terminal. Keyboard and mouse events go through the terminal protocol instead of a browser overlay.

## Build

```bash
bun run terminal:launcher:build
```

That builds the release binary and installs it here:

```text
~/.grok-mission-control/bin/mc
```

The PTY broker automatically uses the installed binary when it exists. It can also fall back to this build output during development:

```text
terminal/launcher-ratatui/target/release/mc
```

Set `GROK_TERMINAL_USE_LAUNCHER=0` to force the old shell-first behavior.

## Controls

- type characters: filter workspaces by name or path
- `backspace`: edit filter
- `esc`: clear filter, or close launcher when filter is empty
- `up/down`: choose workspace
- `tab` or `left/right`: choose app
- `1`: Grok Build
- `2`: Codex
- `3`: Shell
- `enter`: open
- mouse wheel: move through workspaces
- click an app name: choose app
- click a workspace once: select
- click the selected workspace again: open

Shell replaces the launcher. Codex and Grok Build run as child apps; when they exit, the launcher opens again. If you open Shell and run Codex manually from there, exiting Codex returns to that shell.

Commands can be overridden:

```bash
GROK_TERMINAL_CODEX_COMMAND=codex
GROK_TERMINAL_GROK_COMMAND=grok
```
