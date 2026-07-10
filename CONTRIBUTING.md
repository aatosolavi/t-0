# Contributing

Thanks for helping with Launchpad (repo: mission-control).

## Product rule

Keep the browser page a **terminal surface**. Workspace and agent selection belong in the Ratatui launcher (`mc`), not heavy DOM chrome.

## Dev setup

Requirements:

- Node.js 20+ (PTY broker)
- Bun (HTML server)
- Rust stable via rustup (to rebuild `mc`)
- macOS is the primary target today

```bash
git clone https://github.com/aatosolavi/mission-control.git
cd mission-control
bun install
bun run terminal          # foreground
# or
bun run terminal:install  # build mc + LaunchAgent
```

Open http://127.0.0.1:4321

## Rebuild the launcher only

```bash
bun run terminal:launcher:build
```

## Layout

| Path | Role |
|---|---|
| `terminal/index.html` | Browser UI (xterm.js) |
| `terminal/server.ts` | Bun HTML + uploads (:4321) |
| `terminal/pty-server.mjs` | Node PTY + WebSocket (:4322) |
| `terminal/launcher-ratatui` | Workspace + agent TUI |
| `extension/` | Helium/Chrome new-tab redirect |

## PRs

- Small, focused diffs
- Don’t reintroduce a Next.js dashboard without discussion
- Prefer env config (`MC_*`) over hardcoded personal paths
