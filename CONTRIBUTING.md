# Contributing to T-0

Thanks for helping. This project is intentionally small: a **browser terminal** + **`t0` workspace/agent pad**. Keep that focus.

## Product rule

Keep the browser page a **terminal surface**. Workspace and agent selection belong in the Ratatui launcher (`t0`), not heavy DOM chrome. Do not reintroduce a Next.js dashboard without discussion.

## Dev setup

Requirements:

- Node.js 20+ (PTY broker)
- Bun (HTML server)
- Rust stable via rustup (to rebuild `t0`)
- macOS is the primary target today

```bash
git clone https://github.com/aatosolavi/t-0.git
cd t-0
bun install
bun run terminal          # foreground
# or
bun run terminal:install  # build t0 + LaunchAgent
```

Open http://127.0.0.1:4321

Rebuild launcher only:

```bash
bun run terminal:launcher:build
```

Launcher unit tests:

```bash
cargo test --manifest-path terminal/launcher-ratatui/Cargo.toml
```

CI-equivalent check: `bun run check` (vendor + tsc + shell + data-dir + cargo check).

## Publishing to npm

Package name: **`@aatosolavi/t-0`** (scoped — unscoped `t-0` is rejected by npm as too similar to `t0`).  
CLI bin: **`t-0`**. Native pad binary: **`t0`**.

macOS only (`os: ["darwin"]`). Consumers still need Bun + rustup for `t-0 install`.

### First publish (manual, once)

The package must exist on the registry before [Trusted Publishing](https://docs.npmjs.com/trusted-publishers/) can be linked. Do the first publish from a machine where you can complete npm 2FA with your **passkey**:

```bash
git checkout main
git pull
bun install
bun run prepublishOnly
npm pack --dry-run
npm publish --access public
```

Scoped packages need `--access public` (default is restricted).

Confirm: https://www.npmjs.com/package/@aatosolavi/t-0

### Trusted Publisher (after first publish — passkey once in the browser)

1. Open https://www.npmjs.com/package/@aatosolavi/t-0 → **Settings → Trusted Publisher**
2. Provider: **GitHub Actions**
3. Organization/user: `aatosolavi`
4. Repository: `t-0` (the GitHub repo name)
5. Workflow filename: **`publish-npm.yml`**
6. Save (approve with passkey if asked)

No long-lived `NPM_TOKEN` is required once this is linked. The workflow uses OIDC + provenance.

Optional bootstrap secret: granular **Automation** token as repo secret `NPM_TOKEN` — only if you want token-based CI before/without Trusted Publisher.

### Later releases (GitHub Actions)

1. Bump versions in lockstep: `package.json`, `terminal/launcher-ratatui/Cargo.toml` (+ lockfile), `extension/manifest.json`, `CHANGELOG.md`.
2. Merge to `main` (CI green).
3. Create a GitHub Release / tag `vX.Y.Z` → **Publish npm** workflow runs automatically.
4. Or: **Actions → Publish npm → Run workflow**.

Workflow: `.github/workflows/publish-npm.yml`  
- If `NPM_TOKEN` is set → classic token publish  
- Else → OIDC trusted publishing (`npm publish --access public --provenance`)

## Layout

| Path | Role |
|---|---|
| `terminal/index.html` | Browser UI (xterm.js) |
| `terminal/server.ts` | Bun HTML + uploads (:4321) |
| `terminal/pty-server.mjs` | Node PTY + WebSocket (:4322; browser reaches it same-origin at `/pty`) |
| `terminal/vendor.ts` | xterm bundle entry → `terminal/dist/` (built by `terminal:vendor:build`) |
| `terminal/launcher-ratatui` | T-0 TUI (`t0`) |
| `extension/` | Helium/Chrome new-tab redirect |

## Pull requests

- **Small, focused diffs.** One idea per PR when possible.
- Prefer env/`MC_*` config over hardcoded personal paths.
- Prefer expanding Ratatui primitives over new web chrome.
- If you change the PTY stack, document bind host defaults (`127.0.0.1`).

### AI-assisted contributions (read this)

Open source maintainers are drowning in low-quality agent PRs. T-0 is built *for* tokenmaxxers — that does **not** mean unreviewed agent spam is welcome.

**Allowed**

- Using Claude / Codex / Cursor / Grok / etc. to help write code
- AI-assisted refactors **you have read and understand**
- Disclosing AI use in the PR description (appreciated, not shamed)

**Not allowed**

- Autonomous agents opening PRs without a human who owns the change
- Bulk “drive-by” PRs you have not run or read
- AI-generated issue spam, drive-by style rewrites, or unsolicited large features
- Opening PRs solely to farm contribution graphs

**Rules of thumb (what good looks like)**

1. **You own the PR.** You can explain every line if asked.
2. **You ran it.** At least: build `t0` if touched; open `http://127.0.0.1:4321` if terminal paths changed.
3. **Self-review before request for review.** No “the agent said it works.”
4. **Keep PRs reviewable.** Prefer under a few hundred lines unless coordinated.
5. **No auto-opened PRs** from bots/agents without prior maintainer agreement.

If a PR is clearly unreviewed slop, it may be closed without a long debate. That’s about maintainer time, not ideology.

Inspired by patterns many OSS maintainers are formalizing in 2025–2026 (e.g. Godot’s AI contribution tightening; “if you didn’t read every line, don’t open the PR”).

## Security

This product is a **local shell**. Never change the default bind away from localhost without a clear security discussion. See [SECURITY.md](./SECURITY.md).

## Looking for contributions

Two areas where help is especially welcome:

### 1. Finder-class folder UX in `t0`

Settings already has a first-pass **workspace root** browser (navigate folders, pick a root). We want this to feel closer to a **small Finder replacement** for the launcher — not a full file manager, but something people actually enjoy using every day.

Ideas (open an issue or PR; discuss big changes first):

- Clearer navigation (path segments / breadcrumbs, keyboard polish)
- Quick jumps, recents, favorites, volumes
- Create folder, rename, reveal-in-system-Finder
- Smoother mouse + keyboard parity
- Anything that makes “pick / browse workspace” feel native

Primary code: `terminal/launcher-ratatui` (Ratatui TUI). Stay in the terminal surface — no heavy browser chrome.

### 2. Splash screen + ASCII logo

Cold start shows a short **T-0** splash. If you’re good at **ASCII / ANSI art and terminal animation**, we want you.

Welcome contributions:

- A stronger **ASCII logo** for T-0 (fits the existing bordered panel; light + dark)
- A short **ASCII animation** on splash (keep it snappy — skippable, ~hundreds of ms to low seconds)
- Variants that respect reduced motion / `MC_SPLASH=0`

Keep it **ASCII/ANSI-only** (no bitmap assets in the TUI). Orange accent (`#f97316`) is the brand color. Primary code: splash draw path in `terminal/launcher-ratatui/src/main.rs`.

If you only have a logo mock, open an issue with a fenced code block so we can try it in-terminal.

## Good first contributions

- Docs / README polish
- Extra agent chips (if CLI is real and tested)
- UI polish in `t0` that doesn’t add chrome over the PTY
- Install / LaunchAgent edge cases on macOS
- Linux notes (no full systemd product yet unless you ship it)
- The Finder / splash items above (small, demoable PRs preferred)

Open an issue before large features.
