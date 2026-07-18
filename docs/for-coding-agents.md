# For coding agents — install & operate T-0

This page is the **canonical playbook** for agents (Grok, Claude, Cursor, Codex, etc.) helping a human install or debug **T-0** on macOS.

Also available as a skill: [`.agents/skills/install-t0/SKILL.md`](../.agents/skills/install-t0/SKILL.md) (copy into `~/.agents/skills/install-t0/` for auto-invocation).

## What T-0 is

| Piece | Port / path |
|-------|-------------|
| Browser terminal (xterm + PTY) | https://t0.localhost (portless) · http://127.0.0.1:4321 always works |
| PTY broker (Node) | ws://127.0.0.1:4322 |
| Launcher CLI | `t0` → `~/.t-0/bin/t0` |
| State | `~/.t-0/` |
| Source | https://github.com/aatosolavi/t-0 |

**Local shell only.** Default bind is localhost. Security notes: [SECURITY.md](../SECURITY.md).

**New project (`n` in launcher):** scaffolds a git folder under the workspace root, then runs a **harness-neutral headless init** via the chosen agent (Grok/Codex/Claude/Pi/…). T-0 does not reimplement `/init` — it passes a shared bootstrap prompt; each CLI has an argv recipe.

## Install (do this in order)

### 1. Prerequisites

```bash
git --version && node -v && bun -v && rustup -V
```

Missing Bun → https://bun.sh · Missing rustup → https://rustup.rs (required to build `t0`).

### 2. Preferred — npx

```bash
npx t-0
```

### 3. Or curl

```bash
curl -fsSL https://t-0.dev/install | bash
```

### 4. Or clone

```bash
git clone https://github.com/aatosolavi/t-0.git && cd t-0
bun install && bun run terminal:install
open https://t0.localhost   # or http://127.0.0.1:4321
```

### 5. Verify

```bash
command -v t0 && t0
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:4321
curl -sk -o /dev/null -w "%{http_code}\n" https://t0.localhost   # 200 when portless is set up
```

Expect `t0` on PATH and HTTP **200** when the service is up.

## Daily use (for the human)

| Action | How |
|--------|-----|
| Open terminal | https://t0.localhost (fallback http://127.0.0.1:4321) |
| Pick repo + agent | `t0` in any terminal |
| Resume last | `.` in the launcher (filter empty) |
| Favorite | `space` (filter empty) — `★` section at top |
| New project | `n` — scaffold + optional headless agent init |
| Settings | `s` — splash, default agent, IDE, theme, workspace root |
| Help overlay | `?` |
| Screenshot mode | `MC_DEMO=1 t0` or `MC_MOCK=1 t0` (fake repos under `~/work/…`; same list sections as real mode) |

**List sections (top → bottom):** ★ favorites → recent → last → root → scan under workspace root.

## Env vars agents may set (with consent)

| Variable | Purpose |
|----------|---------|
| `MC_WORKSPACE_ROOT` | Root folder of git repos to scan |
| `MC_DATA_DIR` | Override state directory (default `~/.t-0`) |
| `MC_BIND_HOST` | Keep `127.0.0.1` unless user insists otherwise |
| `MC_SPLASH=0` | Skip splash |
| `MC_DEMO=1` / `MC_MOCK=1` | Fake workspaces for screenshots only |
| `MC_USE_LAUNCHER=0` | Shell-first (skip launcher; alias `GROK_TERMINAL_USE_LAUNCHER`) |
| `MC_SESSION_RETAIN_MS` | Idle PTY retain (default 6 h) |

## Repo layout (if editing T-0 itself)

See [AGENTS.md](../AGENTS.md). Short version:

- `terminal/index.html` — browser UI  
- `terminal/server.ts` — Bun HTML + `/pty` proxy + attachments (:4321)  
- `terminal/pty-server.mjs` — Node PTY (:4322)  
- `terminal/launcher-ratatui` — `t0` TUI  
- `terminal/vendor.ts` — xterm bundle entry (`bun run terminal:vendor:build` → `terminal/dist/`)  

Do **not** reintroduce a Next.js dashboard without explicit product direction.

## Dev commands (editing T-0)

```bash
bun run terminal              # foreground stack
bun run terminal:install      # rebuild t0 + LaunchAgent
bun run check                 # vendor + tsc + shell + data-dir + cargo check
cargo test --manifest-path terminal/launcher-ratatui/Cargo.toml
```

## Failure modes

1. **rustup missing** — install; `install.sh` will fail clearly.  
2. **LaunchAgent not running** — `bun run terminal` for foreground logs.  
3. **Wrong workspace root** — Settings → Workspace root, or `MC_WORKSPACE_ROOT`.  
4. **Stale `mc` only** — re-run `bun run terminal:launcher:install` for `t0` + PATH shim.
5. **`https://t0.localhost` dead but `:4321` fine** — `bunx portless proxy start`, then `bunx portless doctor`.
6. **Lag / flicker after upgrade** — hard-reload the browser tab (sessions retain; client must pick up new page assets). Do not “fix” by restarting in a loop if history replay is still in flight.

## Copy this skill into an agent host

```bash
mkdir -p ~/.agents/skills/install-t0
cp /path/to/t-0/.agents/skills/install-t0/SKILL.md ~/.agents/skills/install-t0/
```

Or keep the clone and point the host at `.agents/skills/install-t0/`.

Trigger phrases: “install T-0”, “set up browser terminal”, “install t0”, “mission control terminal”.
