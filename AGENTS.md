# T-0 — agent notes

Product UI name: **T-0**. GitHub repo: `t-0`. CLI: **`t0`** (legacy alias `mc`). State dir: `~/.t-0`.

This repo is **browser terminal only** (not a Next.js app).

**Installing T-0 for a user (not editing this repo):** follow [docs/for-coding-agents.md](./docs/for-coding-agents.md) or the skill [.agents/skills/install-t0/SKILL.md](./.agents/skills/install-t0/SKILL.md).

## What matters

- **Product surface:** `https://t0.localhost` (portless proxy; `http://127.0.0.1:4321` direct) — full-page xterm + real local PTY. Browser connects same-origin at `/pty`; server.ts proxies to the broker.
- **PTY broker:** `terminal/pty-server.mjs` on `127.0.0.1:4322` (must run under Node)
- **HTML server:** `terminal/server.ts` on `:4321` (Bun; re-reads `index.html` each request)
- **Process supervisor:** `terminal/start.mjs` (LaunchAgent entry)
- **T-0 TUI (`t0`):** `terminal/launcher-ratatui` → data-dir `bin/t0` (legacy `bin/mc` shim)

## Commands

```bash
bun install
bun run terminal              # dev / foreground
bun run terminal:install      # rebuild t0 + reinstall LaunchAgent
```

## Config

- `MC_WORKSPACE_ROOT` — where `t0` scans for git repos
- `MC_DATA_DIR` — state/logs/bin (default `~/.t-0`; legacy `~/.mission-control` / `~/.grok-mission-control` auto-migrated)
- `MC_BIND_HOST` — default `127.0.0.1`
- `MC_DEMO=1` / `MC_MOCK=1` — fake public-looking workspaces for marketing screenshots (`MC_DEMO=1 t0`); skips splash

## Conventions

- Keep the browser page a **terminal surface** — no heavy web chrome over the PTY
- Workspace/app selection lives in the **Ratatui launcher**, not DOM overlays
- Prefer `MC_*` env vars over hardcoded personal paths
- LaunchAgent label: `com.mission-control.terminal`

## Do not reintroduce without intent

- Next.js dashboard / `app/` routes
- Orchestrator / ACP harness (agents launch as child CLIs via `t0`)
