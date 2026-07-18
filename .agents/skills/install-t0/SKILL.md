---
name: install-t0
description: >
  Install and verify T-0 (browser terminal + t0 agent/workspace launcher) on macOS.
  Use when the user asks to install T-0, set up mission-control terminal, install t0,
  open a browser terminal with a real local PTY, or run /install-t0.
metadata:
  short-description: "Install T-0 browser terminal + t0 launcher (macOS)"
---

# Install T-0

T-0 is a **local-first** browser terminal (real PTY) plus a Ratatui launcher (`t0`) to pick workspaces and coding agents.

- **Site / install:** https://t-0.dev · `npx t-0`  
- **Repo:** https://github.com/aatosolavi/t-0  
- **Product UI:** https://t0.localhost (portless; fallback http://127.0.0.1:4321)  
- **CLI:** `t0` (legacy alias `mc`)  
- **State dir:** `~/.t-0`  

This product is a **full shell as the user**. Bind stays on **localhost**. Never expose 4321/4322.

## Prerequisites (macOS)

Confirm each exists; install only what’s missing:

| Tool | Check | Install hint |
|------|--------|----------------|
| git | `git --version` | Xcode CLT / brew |
| Node 24+ | `node -v` | `brew install node` (20+ runs T-0; 24+ needed for the https://t0.localhost proxy) |
| Bun | `bun -v` | https://bun.sh |
| rustup | `rustup -V` | https://rustup.rs (needed to **build** `t0`) |

## Preferred install

```bash
npx t-0
```

Or:

```bash
curl -fsSL https://t-0.dev/install | bash
```

This clones to `~/dev/t-0` (or `~/t-0`), runs `bun install`, builds `t0`, installs LaunchAgent `com.mission-control.terminal`, and opens the UI.

**Overrides:**

```bash
MC_INSTALL_DIR=~/src/t-0 MC_REPO_URL=https://github.com/aatosolavi/t-0.git bash install.sh
```

## From an existing clone

```bash
git clone https://github.com/aatosolavi/t-0.git
cd t-0
bun install
bun run terminal:install    # build t0 + LaunchAgent
open https://t0.localhost   # or http://127.0.0.1:4321
```

Dev / foreground (no LaunchAgent):

```bash
bun run terminal
```

Rebuild launcher only:

```bash
bun run terminal:launcher:install
```

## Verify

```bash
command -v t0
t0                          # launcher TUI (or mc legacy alias)
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:4321   # expect 200 when service is up
curl -sk -o /dev/null -w "%{http_code}\n" https://t0.localhost   # 200 once portless is set up (install.sh does this)
```

Logs: `~/.t-0/logs/`

LaunchAgent:

```bash
launchctl kickstart -k "gui/$(id -u)/com.mission-control.terminal"
```

## Config agents should respect

| Env | Meaning |
|-----|---------|
| `MC_WORKSPACE_ROOT` | Where `t0` scans for git repos (or set in Settings → workspace root) |
| `MC_DATA_DIR` | State/logs/bin (default `~/.t-0`) |
| `MC_BIND_HOST` | Default `127.0.0.1` only |
| `MC_SPLASH=0` | Skip cold-start splash |
| `MC_DEMO=1` / `MC_MOCK=1` | Fake workspaces for screenshots (not for daily use) |

Do **not** change bind to `0.0.0.0` without explicit user consent and `MC_ALLOW_REMOTE_BIND=1`.

## What not to do

- Do not reintroduce a Next.js dashboard over the PTY.
- Do not install into random system paths; use the install script or the clone + `terminal:install`.
- Do not commit secrets or personal `MC_*` paths into the user’s projects.
- Do not run `MC_DEMO=1` as the default daily launcher.

## After install — useful commands for the user

| Goal | Command / URL |
|------|----------------|
| Open terminal tab | https://t0.localhost (fallback http://127.0.0.1:4321) |
| Workspace + agent pad | `t0` (list: ★ favorites → recent → last → root → scan) |
| Resume / favorite / new / settings | `.` · `space` · `n` · `s` · `?` help |
| Helium Cmd+T | Load `extension/` as unpacked Chrome/Helium extension |
| Docs | https://github.com/aatosolavi/t-0/blob/main/docs/browser-terminal.md |

## Troubleshooting (quick)

| Symptom | Check |
|---------|--------|
| Page won’t load | LaunchAgent running? `bun run terminal` in foreground for logs |
| No `t0` on PATH | `~/.local/bin` on PATH? Re-run `bun run terminal:launcher:install` |
| Empty workspace list | Settings → workspace root, or `MC_WORKSPACE_ROOT` |
| Colors wrong in Terminal.app | Prefer Ghostty / browser tab for truecolor; not a failed install |

## Success criteria

1. `http://127.0.0.1:4321` serves the terminal (and `https://t0.localhost` once portless is set up).  
2. `t0` runs and lists workspaces (or demo with `MC_DEMO=1`).  
3. User can launch at least Shell (9) or an installed agent CLI.
