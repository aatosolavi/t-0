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
bun run check                 # CI-equivalent: vendor + tsc + shell + data-dir + cargo check
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

## Version numbers (logic)

Semver `MAJOR.MINOR.PATCH`, currently on **0.x**.

| When | Bump |
|------|------|
| Bugfix, docs, polish, small internal change | **PATCH** (`0.2.0` → `0.2.1`) |
| New capability users will notice (feature, new URL/path, real behavior change) | **MINOR** (`0.2.0` → `0.3.0`) |
| Hard break with no compatibility | **MAJOR** (rare while `0.x`) |

**Default when unsure → PATCH.** Prefer small bumps; don’t jump MINOR for a grab-bag of polish. Once a version is tagged/released, don’t rewrite it — next change gets the next number.

Keep these aligned on a release: `package.json`, `terminal/launcher-ratatui/Cargo.toml` (+ lockfile), `extension/manifest.json` if touched, `CHANGELOG.md`, git tag `vX.Y.Z`, GitHub release.

## Do not reintroduce without intent

- Next.js dashboard / `app/` routes
- Orchestrator / ACP harness (agents launch as child CLIs via `t0`)
