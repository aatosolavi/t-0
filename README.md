# Mission Control

**A real local terminal in a browser tab** — plus a Finder-style launcher for workspaces and coding agents.

Open a tab. Get a full PTY. Pick a workspace. Launch Claude, Codex, Pi, Grok, Amp, Devin, Droid, or a plain shell.

> Finder for workspaces — double-click opens an agent, not a folder.

![Mission Control](https://img.shields.io/badge/local--first-orange) ![MIT](https://img.shields.io/badge/license-MIT-blue)

## Install (macOS)

### One-liner

```bash
curl -fsSL https://raw.githubusercontent.com/aatosolavi/mission-control/main/install.sh | bash
```

### From a clone

```bash
git clone https://github.com/aatosolavi/mission-control.git
cd mission-control
bun install
bun run terminal:install   # build `mc` + LaunchAgent
open http://127.0.0.1:4321
```

### Requirements

| Tool | Why |
|---|---|
| **Node.js 20+** | PTY broker (`@lydell/node-pty`) |
| **Bun** | Tiny HTML server |
| **Rust / rustup** | Build the `mc` launcher (prebuilt binaries planned) |
| **macOS** | LaunchAgent install path (Linux/Windows later) |

Foreground without installing the agent:

```bash
bun run terminal
# → http://127.0.0.1:4321
```

## What you get

| Piece | Role |
|---|---|
| Browser UI | xterm.js full-page terminal, light/dark/system, orange accent |
| PTY broker | Real shell over WebSocket on **127.0.0.1:4322** |
| `mc` launcher | Filter `~/dev` (or `MC_WORKSPACE_ROOT`), pick agent, go |
| Sessions | Reload reattaches; idle retain across laptop sleep |
| Helium extension | `extension/` → Cmd+T becomes a terminal |

### Launcher (`mc`)

| Key | App |
|-----|-----|
| 1 | Grok |
| 2 | Codex |
| 3 | Pi |
| 4 | Claude |
| 5 | Amp |
| 6 | Devin |
| 7 | Droid |
| 8 | Shell |

Missing CLIs are **dimmed**. From any shell: run `mc` again.

**Memory (tokenmaxxer mode):**
- Remembers **last agent per workspace** (auto-selects when you highlight a repo)
- **`space`** (empty filter) toggles **favorite** — favorites float to the top (`★`)
- **`.`** (empty filter) **continues last** workspace + agent

**Side actions** (filter empty): **`e`** editor · **`f`** Finder · **`c`** copy path · **`g`** GitHub  

**Git rows:** branch name, `*` if dirty, `↑N` if ahead of upstream; remembered agent shown on the row.

### Themes

- `?theme=light` · `?theme=dark` · `?theme=system`
- **⌘/Ctrl+Shift+L** cycles (saved in `localStorage`)

### Config (env)

| Variable | Default |
|---|---|
| `MC_WORKSPACE_ROOT` | `~/dev` if it exists, else `$HOME` |
| `MC_DATA_DIR` | `~/.mission-control` (or legacy `~/.grok-mission-control` if present) |
| `MC_BIND_HOST` | `127.0.0.1` |
| `MC_LAUNCHER` / `GROK_TERMINAL_*_COMMAND` | paths to `mc` and agent CLIs |

Data, logs, attachments, and the `mc` binary live under the data dir.

## Helium / Chrome new tab

1. `chrome://extensions` → Developer mode  
2. Load unpacked → select `extension/`  
3. Cmd+T → Mission Control (when the server is running)

## Security

This is a **full shell as your user** on localhost. Do not expose ports 4321/4322 to the network. See [SECURITY.md](./SECURITY.md).

## Docs

- [docs/browser-terminal.md](./docs/browser-terminal.md) — PTY notes, sessions, launcher  
- [CONTRIBUTING.md](./CONTRIBUTING.md) — how to hack  
- [terminal/launcher-ratatui/README.md](./terminal/launcher-ratatui/README.md) — `mc` keys  

## Stack

- **xterm.js** (CDN) · **@lydell/node-pty** · **ws**
- **Bun** HTML server · **Node** PTY broker
- **Rust / Ratatui** launcher

## Status

Terminal-first, local-first, open source (MIT). Built for people who live in agent CLIs and want tabs that feel like Finder for work.

## License

[MIT](./LICENSE)
