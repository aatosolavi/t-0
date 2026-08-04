# Changelog

## [0.3.0] — 2026-08-04

### 🚀 npm package
- Publish **`t-0`** on the public npm registry (`npm install -g t-0`)
- CLI: `t-0 install` · `t-0 start` · `t-0 doctor` · `t-0 version` (native pad remains `t0`)
- Package root resolution works for global installs (not `process.cwd()`)
- `files` whitelist + `prepublishOnly` vendor build; macOS (`os: darwin`), Node ≥ 20
- **GitHub Actions publish** (`.github/workflows/publish-npm.yml`) on Release — Automation token or OIDC trusted publishing (passkey-friendly; no local OTP)

### Docs
- Install via npm alongside curl one-liner and git clone
- CONTRIBUTING: npm CI publish setup

## [0.2.3] — 2026-08-04

### 🛠 Fixes / ops
- **Portless repair path:** `bun run portless:repair` re-registers `t0 → 4321`, starts the HTTPS proxy, and runs `portless doctor` when `https://t0.localhost` is down but `:4321` is fine
- Agent docs: diagnose `:4321` vs portless separately; reinstall the portless login service when the proxy does not survive reboot

### 📦 Dependencies
- `portless` 0.15.1 → 0.15.5
- `@lydell/node-pty` 1.2.0-beta.12 → 1.2.0-beta.14
- `ws` 8.21.0 → 8.21.2
- `@types/node` 26.1.1 → 26.1.2

## [0.2.2] — 2026-07-13

### 🛠 Fixes
- **Web TUI crash-loop:** drop `terminal.clear()` at startup — ratatui 0.30 waits on cursor-position (`ESC[6n`) and can exit after ~2 s while the browser is still replaying session history; that respawned `t0`, grew history, and looked like lag / cursor flash
- **Web paint lag:** coalesce PTY→WebSocket output (and batch `term.write` in the page) so a full-frame paint is one write, not dozens of ~1 KB frames
- Exit cleanly when stdin is not a TTY / poll·read fails (no busy-loop after broker death)
- Draw only when something visible changed; first frame still unconditional
- Fixed-height picker panel + scroll math that accounts for section separators
- Cursor DOM probe less aggressive (no per-message sync)

## [0.2.1] — 2026-07-13

### 🚀 Launch pad
- **New Project** popup — scaffold a repo + optional harness-neutral headless agent init (stays in the TUI)
- Multi-line notes (Shift+Enter), content-sized popup, Finder-style parent picker

### ✨ Launcher UI
- Selection is unmissable: full-width surface + `▌` accent bar
- Color discipline: orange = interaction only; remembered agents calm; dirty git uses amber warn
- Shared taller panel for picker · settings · folder browser
- Section separators instead of noisy badge column
- Honest `…` truncation, filter caret + live count + bold matches
- One live status line + `?` keymap overlay; scroll ▲/▼; empty states

### 🎬 Motion (silence at rest)
- Tips row above the stable keymap — typewriter reveal, sparkle, orange color ramp (~30 s, preemptible)
- Braille spinner only while install/init jobs run
- One-frame `T-0 · liftoff` brand paint on launch (zero delay)
- `✦ created` sparkle after new project

### ⚡ Performance
- Drain all pending keys/mouse before one draw (paste + drag feel instant)
- Paint workspaces first; git badges fill in async (dead mounts can’t hang the UI)
- Discovery overlaps splash; re-discover when returning from an agent
- Broker history as chunk list + coalesce (no quadratic string copy)
- Skip idle `lsof` when no clients / no PTY output
- Drop 80 ms artificial delay on browser tab start

### 🛠 Fixes
- Event-loop `continue` no longer renders one keystroke late
- Settings mouse: click to select, second click to activate
- Demo mode keeps baked git badges for screenshots
- Stuck git inspect abandons after 10 s (no forever-40 ms poll)

## [0.2.0] — 2026-07-11

### Highlights
- **Portless URL:** `https://t0.localhost` as the standard product URL (installer sets up portless)
- **Same-origin PTY proxy** — browser talks to `/pty` instead of a separate :4322 origin by default
- **State dir:** `~/.t-0` (auto-migrates `~/.mission-control` / `~/.grok-mission-control`)
- **Agent skill:** `.agents/skills/install-t0` for coding agents installing T-0

### Browser terminal
- Vendor **xterm** locally (no CDN dependency for core UI)
- Harden HTML server
- Web links, WebGL renderer option, font-size keys, bell ping, auto-reconnect
- Dependency bumps (xterm 6, types, etc.)

### Launcher / product
- CLI remains **`t0`** (legacy `mc` alias)
- Install skill under `.agents/skills` (not `.grok`)

## [0.1.0] — 2026-07-11

First public cut: browser terminal + Ratatui launcher, themes, demo mode, Finder-style workspace root picker, install script, LaunchAgent, docs and screenshot.
