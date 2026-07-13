# T-0 (Ratatui)

Native **T-0** UI for the browser terminal — pick a workspace, launch an agent.

It runs inside the PTY, so xterm.js only has to be a terminal. Keyboard and mouse events go through the terminal protocol instead of a browser overlay.

## Build

```bash
bun run terminal:launcher:build
```

That builds the release binary and installs it under the data dir:

```text
~/.t-0/bin/t0
```

(Legacy PATH alias: `mc` → same binary.)

The PTY broker automatically uses the installed binary when it exists. It can also fall back to this build output during development:

```text
terminal/launcher-ratatui/target/release/t0
```

Set `GROK_TERMINAL_USE_LAUNCHER=0` or `MC_USE_LAUNCHER=0` to force the old shell-first behavior.

Cold-start splash (“T-0 · go for launch”, once per `t0` process, skipped when returning from an agent). Disable with `MC_SPLASH=0`.

Marketing screenshots: `MC_DEMO=1 t0` (or `MC_MOCK=1`) uses fake public-looking workspaces under `~/work/…` and skips the splash.

## Controls

- type characters: filter workspaces by name or path
- `backspace`: edit filter
- `esc`: clear filter, or close launcher when filter is empty
- `up/down`: choose workspace
- `tab` or `left/right`: choose app
- `1`: Grok
- `2`: Codex
- `3`: Pi
- `4`: Cursor (runs `agent` / cursor-agent CLI)
- `5`: Claude
- `6`: Amp
- `7`: Devin
- `8`: Droid
- `9`: Shell
- `enter`: open
- `.` (filter empty): resume last workspace + agent
- `space` (filter empty): toggle favorite (`★` at top)
- `e` (filter empty): open folder in **Cursor.app / IDE** (not the agent)
- `f` (filter empty): open in Finder
- `c` (filter empty): copy path
- `g` (filter empty): open GitHub / origin remote
- `s` (filter empty): settings (splash, default agent, UI theme, workspace root)
- `n` (filter empty): **new project** popup — name, parent, template, init agent, optional notes; scaffolds a git repo then runs **headless init** via the chosen agent (harness-neutral recipes for Grok, Codex, Claude, Pi, Amp, Devin, Droid, Cursor)
- rows show git branch (`*` dirty, `↑N` ahead) and remembered agent
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
