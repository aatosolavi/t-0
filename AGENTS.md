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
cargo test --manifest-path terminal/launcher-ratatui/Cargo.toml
```

## Config

- `MC_WORKSPACE_ROOT` — where `t0` scans for git repos
- `MC_DATA_DIR` — state/logs/bin (default `~/.t-0`; legacy `~/.mission-control` / `~/.grok-mission-control` auto-migrated)
- `MC_BIND_HOST` — default `127.0.0.1`
- `MC_SPLASH=0` — skip cold-start splash (also Settings)
- `MC_DEMO=1` / `MC_MOCK=1` — fake public-looking workspaces for marketing screenshots (`MC_DEMO=1 t0`); skips splash; **same list section order** as real discovery
- `MC_SESSION_RETAIN_MS` — idle PTY retain (default 6 h; alias `GROK_TERMINAL_SESSION_RETAIN_MS`)
- `MC_UI_THEME` — set by the PTY broker from browser theme for launcher `auto` mode (not usually set by hand)

## Launcher list sections (order)

★ favorites → recent → last → root → scan under workspace root (label = root path). Keys: enter open, `.` resume, space favorite, 1–9 agent, n new project, s settings, ? help, type to filter.

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

## Terminal motion budget (t0 launcher)

- **One live region.** The status line is the only place that changes without input at rest. Tips, flashes, and job-adjacent copy compete for that slot — never two independent moving decorations at once.
- **One-shot animations:** ≤300 ms, 3–5 frames, only in response to a user action.
- **Continuous animation** only while a real background job runs (install / headless init). Spinner lives on the job bar; tips pause while jobs run.
- **Idle:** never faster than ~one redraw per 30 s for decoration (tips). Faster idle loops burn battery for no product value.
- **Fake fade:** terminals cannot alpha-fade; use a 3-step color ramp (dim → muted → text) over 3 frames at 40 ms poll. Reuse the status active path.
- **Do not:** smooth-scroll selection, pulse dirty `*` or any per-row idle motion, idle easter eggs that force continuous redraw, or animate two places simultaneously. Do not delay the launch/exec path for branding.

## Cursor Cloud specific instructions

Cloud agents use `.cursor/environment.json`. Snapshot should include Bun (`~/.bun`), Node 22, and Rust stable. After boot, `install` runs `bun install --frozen-lockfile` and `bun run terminal:launcher:build` (fails fast if `bun` is missing — no remote bootstrap).

```bash
export PATH="$HOME/.bun/bin:$HOME/.local/bin:$PATH"
bun run check
cargo test --manifest-path terminal/launcher-ratatui/Cargo.toml
# live stack (auto-started via environment terminals; PTY then execs installed ~/.t-0/bin/t0):
bun run terminal   # http://127.0.0.1:4321 — not `t0` directly
```

Notes:
- `bun run terminal:install` installs a macOS LaunchAgent — skip on Linux cloud VMs; use `bun run terminal` / `bun run terminal:launcher:build` instead.
- `t0` needs a real TTY; non-interactive `t0` may exit with ENXIO — expected. Prefer `cargo test` / `bun run check` for verification.
- Rebuild launcher after Rust edits: `bun run terminal:launcher:build` (also done by cloud `install`).

## Do not reintroduce without intent

- Next.js dashboard / `app/` routes
- Orchestrator / ACP harness (agents launch as child CLIs via `t0`)
