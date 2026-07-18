# T-0

**A real local terminal in a browser tab** — plus a launcher for workspaces and coding agents.

Open a tab. Get a full PTY. Pick a workspace. Launch Claude, Codex, Pi, Cursor, Grok, Amp, Devin, Droid, or a plain shell.

> T-0 — countdown done. Workspaces in, agents out.

[![local-first](https://img.shields.io/badge/local--first-orange)](https://github.com/aatosolavi/t-0)
[![MIT](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)
[![macOS](https://img.shields.io/badge/platform-macOS-lightgrey)](#install-macos)
[![CLI](https://img.shields.io/badge/CLI-t0-f97316)](#t-0-keys-t0)

**Repo:** [aatosolavi/t-0](https://github.com/aatosolavi/t-0) · **CLI:** `t0` (legacy alias `mc`) · **State:** `~/.t-0`

<p align="center">
  <img
    src="https://raw.githubusercontent.com/aatosolavi/t-0/main/docs/assets/launchpad.png"
    alt="T-0 launcher — pick a workspace and agent (demo data)"
    width="920"
  />
</p>

<p align="center"><sub>T-0 launcher (<code>t0</code>)</sub></p>

## Install (macOS)

Site: [t-0.dev](https://t-0.dev)

### Preferred — npx

```bash
npx t-0
```

Installs (or opens) T-0. Requires git, Node, Bun, and rustup.

### Curl

```bash
curl -fsSL https://t-0.dev/install | bash
```

### From a clone

```bash
git clone https://github.com/aatosolavi/t-0.git
cd t-0
bun install
bun run terminal:install   # build t0 + LaunchAgent
open https://t0.localhost   # or http://127.0.0.1:4321
```

### Requirements

| Tool | Why |
|---|---|
| **Node.js 24+** | PTY broker (`@lydell/node-pty`); 20+ runs T-0, 24+ needed for the `t0.localhost` proxy |
| **Bun** | Tiny HTML server |
| **Rust / rustup** | Build the `t0` launcher (prebuilt binaries planned) |
| **macOS** | LaunchAgent install path (Linux/Windows later) |

Foreground without installing the agent:

```bash
bun run terminal
# → http://127.0.0.1:4321
```

Then in any shell:

```bash
t0                 # open the workspace / agent pad
MC_DEMO=1 t0       # same UI with fake public-looking repos (for screenshots)
```

## What you get

| Piece | Role |
|---|---|
| **Browser UI** | Full-page xterm.js, light/dark/system, orange accent |
| **PTY broker** | Real local shell over WebSocket on **127.0.0.1:4322** (page reaches it same-origin at `/pty`) |
| **`t0`** | Workspace list + agent pad inside the PTY |
| **Sessions** | Reload reattaches; idle retain across laptop sleep |
| **Helium extension** | `extension/` → Cmd+T becomes a terminal |

### Workspace list (`t0`)

Sections, top to bottom:

1. **★ favorites**
2. **recent**
3. **last** (last session / cwd)
4. **root** (the workspace root itself, if it is a repo)
5. **scan** under the workspace root (section label = that path)

Type to filter by name or path. **Git rows:** branch · `*` dirty · `↑N` ahead · remembered agent. Selection is a full-width surface with a `▌` accent bar.

Demo mode (`MC_DEMO=1 t0` / `MC_MOCK=1`) uses the **same section order** with fake repos under `~/work/…` and skips splash.

### T-0 keys (`t0`)

| Key | Does |
|-----|------|
| **enter** | Open selected workspace with agent |
| **`.`** | Resume last workspace + agent (filter empty) |
| **space** | Toggle favorite (`★`) (filter empty) |
| **1–9** | Pick agent chip |
| **n** | New Project popup |
| **s** | Settings |
| **?** | Full keymap overlay |
| type | Filter by name or path · **esc** clears |
| **tab** / ←→ | Move between agent chips |
| ↑↓ / wheel | Move through workspaces |

**Agents (1–9)**

| Key | App |
|-----|-----|
| 1 | Grok |
| 2 | Codex |
| 3 | Pi |
| 4 | Cursor (agent CLI) |
| 5 | Claude |
| 6 | Amp |
| 7 | Devin |
| 8 | Droid |
| 9 | Shell |

Missing CLIs are **dimmed**. **Hover** a dim chip (or press its number / enter) to **install** npm-backed agents (Codex, Claude, Pi) when a recipe is known. Script-based installers need `MC_ALLOW_SCRIPT_INSTALL=1`.

**New Project (`n`)** — scaffold a git repo under a chosen parent, optional headless agent init (Grok / Codex / Claude / …). Multi-line notes with **Shift+Enter**; you stay in the pad while init streams into a job bar.

**Settings (`s`)** — splash, default agent, IDE for `e`, UI theme (`auto` / `dark` / `light`), workspace root picker.

**Side actions** (filter empty)

| Key | Does |
|-----|------|
| **`e`** | Open folder in Cursor/IDE |
| **`f`** | Finder |
| **`c`** | Copy path |
| **`g`** | GitHub origin |

**Memory** — last agent per workspace (auto-select on highlight); favorites / settings in `$MC_DATA_DIR/launcher-state.json`; recents also in `recent-workspaces.txt`.

**Motion** — one live status region (tips ~every 30 s when idle); spinner only while install/init jobs run. Full map: [launcher README](./terminal/launcher-ratatui/README.md).

### Themes

**Browser chrome**
- `?theme=light` · `?theme=dark` · `?theme=system`
- **⌘/Ctrl+Shift+L** cycles

**Launcher (`t0`)** — Settings → UI theme: `auto` / `dark` / `light`

### Stable URL — `https://t0.localhost`

The standard address is **`https://t0.localhost`**, fronted by [portless](https://portless.sh/) (ships as a dev dependency; `portless.json` in the repo). `install.sh` sets it up automatically — route, HTTPS CA trust (one sudo prompt), and a startup service. The PTY websocket is served same-origin at `/pty`, so the one URL carries everything.

Manual setup or repair:

```bash
bunx portless alias t0 4321 && bunx portless proxy start
bunx portless trust            # once; adds the local CA (sudo)
bunx portless service install  # once; start proxy at login
```

No portless (or Node < 24)? Nothing breaks — `http://127.0.0.1:4321` always works.

### Config (env)

| Variable | Default |
|---|---|
| `MC_WORKSPACE_ROOT` | `~/dev` if it exists, else `$HOME` (or path set in Settings) |
| `MC_DATA_DIR` | `~/.t-0` (legacy `~/.mission-control` / `~/.grok-mission-control` auto-migrated) |
| `MC_BIND_HOST` | `127.0.0.1` |
| `MC_SPLASH` | splash on cold start (`0` to disable; also settable in Settings) |
| `MC_DEMO` / `MC_MOCK` | `1` = fake workspaces for marketing screenshots (skips splash) |
| `MC_SESSION_RETAIN_MS` | idle session retain (default 6 h; alias `GROK_TERMINAL_SESSION_RETAIN_MS`) |
| `MC_USE_LAUNCHER` | `0` = shell-first (skip launcher; alias `GROK_TERMINAL_USE_LAUNCHER`) |
| `GROK_TERMINAL_*_COMMAND` | override agent CLI paths |

## For coding agents

If another agent is installing or debugging T-0 for a human:

- Playbook: [docs/for-coding-agents.md](./docs/for-coding-agents.md)
- Skill: [.agents/skills/install-t0/SKILL.md](./.agents/skills/install-t0/SKILL.md)  
  Copy into the host skill dir (e.g. `~/.agents/skills/install-t0/`) for auto-invocation / `/install-t0`.

## Security

This is a **full shell as your user**, bound to **localhost only**. Do not expose ports 4321/4322. See [SECURITY.md](./SECURITY.md).

## Contributing

Ideas and PRs welcome — especially if you live in agent CLIs too.

**Actively looking for help on:**

1. **Finder-class UX in `t0`** — make the workspace/folder browser feel like a small Finder replacement for the launcher (navigation, jumps, keyboard/mouse polish).
2. **Splash + logo (ASCII)** — terminal splash animation and a solid T-0 ASCII logo. ANSI art welcome; keep it skippable and light/dark friendly.

Details: [CONTRIBUTING.md](./CONTRIBUTING.md). Short version: **humans own PRs**; AI help is fine if you reviewed and ran the change. Unreviewed agent spam will be closed.

## Docs

- [docs/browser-terminal.md](./docs/browser-terminal.md) — PTY notes, sessions, launcher  
- [CONTRIBUTING.md](./CONTRIBUTING.md) · [SECURITY.md](./SECURITY.md)  
- [terminal/launcher-ratatui/README.md](./terminal/launcher-ratatui/README.md) — full key map  

## Stack

- **xterm.js** (locally bundled) · **@lydell/node-pty** · **ws**
- **Bun** HTML server · **Node** PTY broker
- **Rust / Ratatui** T-0 UI (`t0`)

## License

[MIT](./LICENSE)
