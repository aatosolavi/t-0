# Changelog

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
