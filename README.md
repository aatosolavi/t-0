# T-0

**A real local terminal in a browser tab** — plus a launcher for workspaces and coding agents.

Open a tab. Get a full PTY. Pick a workspace. Launch Claude, Codex, Pi, Cursor, Grok, Amp, Devin, Droid, or a plain shell.

> T-0 — countdown done. Workspaces in, agents out.

[![local-first](https://img.shields.io/badge/local--first-orange)](https://github.com/aatosolavi/t-0)
[![MIT](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)
[![macOS](https://img.shields.io/badge/platform-macOS-lightgrey)](#install-macos)
[![CLI](https://img.shields.io/badge/CLI-t0-f97316)](#t-0-keys-t0)

**Repo:** [aatosolavi/t-0](https://github.com/aatosolavi/t-0) · **CLI:** `t0` (legacy alias `mc`) · **State:** `~/.mission-control`

<p align="center">
  <img
    src="https://raw.githubusercontent.com/aatosolavi/t-0/main/docs/assets/launchpad.png"
    alt="T-0 launcher — pick a workspace and agent (demo data)"
    width="920"
  />
</p>

<p align="center"><sub>Launcher UI (<code>MC_DEMO=1 t0</code> — mock workspaces for screenshots; your machine shows your own repos)</sub></p>

## Install (macOS)

### One-liner

```bash
curl -fsSL https://raw.githubusercontent.com/aatosolavi/t-0/main/install.sh | bash
```

### From a clone

```bash
git clone https://github.com/aatosolavi/t-0.git
cd t-0
bun install
bun run terminal:install   # build t0 + LaunchAgent
open http://127.0.0.1:4321
```

### Requirements

| Tool | Why |
|---|---|
| **Node.js 20+** | PTY broker (`@lydell/node-pty`) |
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
| **PTY broker** | Real local shell over WebSocket on **127.0.0.1:4322** |
| **`t0`** | Filter workspaces, pick agent, go |
| **Sessions** | Reload reattaches; idle retain across laptop sleep |
| **Helium extension** | `extension/` → Cmd+T becomes a terminal |

### T-0 keys (`t0`)

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

**Memory**
- Last agent per workspace (auto-select on highlight)
- **`space`** — favorite (`★` floats to top)
- **`.`** — resume last workspace + agent

**Side actions** (filter empty)

| Key | Does |
|-----|------|
| **`e`** | Open folder in Cursor/IDE |
| **`f`** | Finder |
| **`c`** | Copy path |
| **`g`** | GitHub origin |
| **`s`** | Settings (splash, default agent, IDE for `e`, UI theme, workspace root picker) |

**Git rows:** branch · `*` dirty · `↑N` ahead · remembered agent on the row.

### Themes

**Browser chrome**
- `?theme=light` · `?theme=dark` · `?theme=system`
- **⌘/Ctrl+Shift+L** cycles

**Launcher (`t0`)** — Settings → UI theme: `auto` / `dark` / `light` (truecolor looks best in Ghostty / the browser tab).

### Config (env)

| Variable | Default |
|---|---|
| `MC_WORKSPACE_ROOT` | `~/dev` if it exists, else `$HOME` (or path set in Settings) |
| `MC_DATA_DIR` | `~/.mission-control` (legacy `~/.grok-mission-control` is auto-migrated) |
| `MC_BIND_HOST` | `127.0.0.1` |
| `MC_SPLASH` | splash on cold start (`0` to disable) |
| `MC_DEMO` / `MC_MOCK` | `1` = fake workspaces for marketing screenshots |
| `GROK_TERMINAL_*_COMMAND` | override agent CLI paths |

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

- **xterm.js** (CDN) · **@lydell/node-pty** · **ws**
- **Bun** HTML server · **Node** PTY broker
- **Rust / Ratatui** T-0 UI (`t0`)

## License

[MIT](./LICENSE)
