# Security

T-0 is a **local full shell** in the browser.

## Threat model

- Anyone who can use the HTML UI **and** the PTY WebSocket as your user can run commands **as your user**.
- Treat ports **4321/4322** like an unlocked terminal window.

## Defaults (keep these)

| Control | Behavior |
|---|---|
| Bind | HTML + PTY default to **127.0.0.1** only |
| Remote bind | Refused unless `MC_ALLOW_REMOTE_BIND=1` |
| PTY WebSocket | **Origin allowlist** — only local UI origins (`http://127.0.0.1:4321`, `http://localhost:4321`, plus `MC_ALLOWED_ORIGINS`) |
| No-Origin clients | Denied unless `MC_ALLOW_NO_ORIGIN=1` |
| Attachments | Same-origin POSTs only; sanitized basenames; max count, file size, and request size |
| Agent install | Default **npm packages only**; `curl \| bash` recipes need `MC_ALLOW_SCRIPT_INSTALL=1` |
| New-project headless init | Init recipes may grant **elevated tool autonomy** (skip prompts / force / auto-write); argv-only (no shell interpolation of notes); **intended** scope is the new project dir via process cwd + prompt — elevated tools may still reach other paths the user can access |

## Cross-site WebSocket (CSWSH)

Browsers can open websockets to localhost from *other* websites. Without Origin checks, a malicious page could talk to the PTY broker.

Mitigation in T-0: reject connections whose `Origin` is not on the allowlist.

## Browser dependencies

The terminal page bundles **xterm.js and its addons locally**. It does not load executable code or fonts from third-party origins. Because browser code can control a full shell, keep executable dependencies same-origin and covered by the page's Content Security Policy.

## Quick self-check

- [x] Default bind is localhost
- [x] PTY Origin allowlist
- [x] Browser dependencies bundled and served locally
- [x] Attachment Origin checks + aggregate request limit
- [x] No API keys / private home paths in tracked source
- [x] Attachment name sanitize + size limits
- [x] Install hover does not silently run unpinned curl scripts by default

## Reporting

Open a private security advisory on the GitHub repo or contact the maintainer via GitHub.
