# Launchpad — agent notes

Product UI name: **Launchpad**. Repo/package/data still `mission-control` / `mc`.

This repo is **browser terminal only** (not a Next.js app).

## What matters

- **Product surface:** `http://127.0.0.1:4321` — full-page xterm + real local PTY
- **PTY broker:** `terminal/pty-server.mjs` on `127.0.0.1:4322` (must run under Node)
- **HTML server:** `terminal/server.ts` on `:4321` (Bun; re-reads `index.html` each request)
- **Process supervisor:** `terminal/start.mjs` (LaunchAgent entry)
- **Launchpad TUI (`mc`):** `terminal/launcher-ratatui` → data-dir `bin/mc`

## Commands

```bash
bun install
bun run terminal              # dev / foreground
bun run terminal:install      # rebuild mc + reinstall LaunchAgent
```

## Config

- `MC_WORKSPACE_ROOT` — where `mc` scans for git repos
- `MC_DATA_DIR` — state/logs/bin (default `~/.mission-control`, legacy `~/.grok-mission-control`)
- `MC_BIND_HOST` — default `127.0.0.1`

## Conventions

- Keep the browser page a **terminal surface** — no heavy web chrome over the PTY
- Workspace/app selection lives in the **Ratatui launcher**, not DOM overlays
- Prefer `MC_*` env vars over hardcoded personal paths
- LaunchAgent label: `com.mission-control.terminal`

## Do not reintroduce without intent

- Next.js dashboard / `app/` routes
- Orchestrator / ACP harness (agents launch as child CLIs via `mc`)
