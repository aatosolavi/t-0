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

Marketing screenshots: `MC_DEMO=1 t0` (or `MC_MOCK=1`) uses fake public-looking workspaces under `~/work/…`, skips the splash, and **keeps the same list section order** as real discovery (no FS scan).

After a successful headless init, the launcher pauses with `init finished — press enter` so the agent summary stays readable. Skip with `MC_INIT_NO_PAUSE=1`. Dry-run recipes with `MC_INIT_DRY_RUN=1` (prints argv, no spawn).

## List sections

Top → bottom (demo matches this order):

1. **★ favorites**
2. **recent**
3. **last** (last session / cwd)
4. **root** (workspace root when it is a repo)
5. **scan** under the workspace root (section label = root path)

Selection is a full-width surface with a `▌` accent bar. Workspaces paint first; git badges fill in async.

## Controls

- type characters: filter workspaces by name or path
- `backspace`: edit filter
- `esc`: clear filter, or close launcher when filter is empty
- `up/down`: choose workspace
- `tab` or `left/right`: choose app
- `1`: Claude
- `2`: Codex
- `3`: Cursor (runs `agent` / cursor-agent CLI)
- `4`: Grok
- `5`: Pi
- `6`: Devin
- `7`: Droid
- `8`: Amp
- `9`: Shell
- `enter`: open
- `.` (filter empty): resume last workspace + agent
- `space` (filter empty): toggle favorite (`★` at top)
- `?` (filter empty): keymap overlay
- `e` (filter empty): open folder in **Cursor.app / IDE** (not the agent)
- `f` (filter empty): open in Finder
- `c` (filter empty): copy path
- `g` (filter empty): open GitHub / origin remote
- `s` (filter empty): settings (splash, default agent, default IDE for `e`, UI theme, workspace root)
- `n` (filter empty): **new project** popup — name, parent, template, init agent, multi-line notes; scaffolds a git repo then runs **headless init in the background** (you stay in the pad; live output streams into a bar under the panel). Harness-neutral recipes for Grok, Codex, Claude, Pi, Amp, Devin, Droid, Cursor. Notes: **Shift+Enter** inserts newline; plain **Enter** advances fields. Create with the **Create** row, or **Ctrl+Enter (in T-0’s browser terminal)**. **Shift+Up** leaves long notes without scrolling. Word/line: **Opt+Backspace** / **Ctrl+W**, **Ctrl+U**. Elevated agents show **full tools**. `MC_INIT_DRY_RUN=1` prints the would-be command instead of spawning.
- rows show git branch (`*` dirty, `↑N` ahead) and remembered agent
- mouse wheel: move through workspaces
- click an app name: choose app
- click a workspace once: select
- click the selected workspace again: open

## Motion (silence at rest)

- **One live region** — status/tips line only; no second continuous decoration
- Idle tips ~every **30 s** (typewriter + short color ramp; preemptible)
- Braille spinner **only** while install / headless-init jobs run
- One-shot sparkles / brand frames ≤ a few hundred ms, never on the launch path delay

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

## Tests

```bash
cargo test --manifest-path terminal/launcher-ratatui/Cargo.toml
```
