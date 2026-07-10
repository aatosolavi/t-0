# Mission Control Ratatui Launcher

This is the native startup screen for the browser terminal.

It runs inside the PTY, so xterm.js only has to be a terminal. Keyboard and mouse events go through the terminal protocol instead of a browser overlay.

## Build

```bash
bun run terminal:launcher:build
```

That builds the release binary and installs it under the data dir:

```text
~/.mission-control/bin/mc
# or legacy: ~/.grok-mission-control/bin/mc
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
- `1`: Grok
- `2`: Codex
- `3`: Pi
- `4`: Claude
- `5`: Amp
- `6`: Devin
- `7`: Droid
- `8`: Shell
- `enter`: open
- `.` (filter empty): continue last workspace + agent
- `space` (filter empty): toggle favorite (`★` at top)
- mouse wheel: move through workspaces
- click an app name: choose app
- click a workspace once: select
- click the selected workspace again: open

Memory lives in `$MC_DATA_DIR/launcher-state.json` (last launch, favorites, per-workspace agent).

Shell replaces the launcher. Agent CLIs (Grok, Codex, Pi, Claude, Amp, Devin, Droid) run as child apps; when they exit, the launcher opens again. If you open Shell and run an agent manually from there, exiting the agent returns to that shell.

Commands can be overridden:

```bash
GROK_TERMINAL_GROK_COMMAND=grok
GROK_TERMINAL_CODEX_COMMAND=codex
GROK_TERMINAL_PI_COMMAND=pi
GROK_TERMINAL_CLAUDE_COMMAND=claude
GROK_TERMINAL_AMP_COMMAND=amp
GROK_TERMINAL_DEVIN_COMMAND=devin
GROK_TERMINAL_DROID_COMMAND=droid
```
