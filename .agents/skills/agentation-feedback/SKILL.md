---
name: agentation-feedback
description: Read and act on Agentation UI comments from the local T-0 website.
  Use when: the user mentions annotations, Agentation, UI comments, "address my
  feedback", "fix annotation", or comments left on the pad landing page in local
  dev. Not the product terminal.
---

# Agentation feedback (local website only)

Toolbar and MCP are **local-dev only** on the marketing pad (`www/public`).
Do not mount Agentation on `terminal/index.html` / t0.localhost.

- Serve: `bun run site:dev` → http://127.0.0.1:5173
- Production `https://t-0.dev` does not load Agentation
- MCP: `npx -y agentation-mcp server` (stdio + HTTP on `http://localhost:4747`)
- Config: `.grok/config.toml` and `.mcp.json`

If Agentation tools are missing in this session: `/mcps` then `r`, or start a new chat in this repo.

## When the user leaves UI comments

1. Call `agentation_get_all_pending` (or `agentation_list_sessions` then `agentation_get_pending`).
2. `agentation_acknowledge` each relevant annotation.
3. Change `www/public/index.html` (and CSS in that file). Use `elementPath` / `cssClasses`.
4. `agentation_resolve` with a short summary, or `agentation_reply` if you need a question.
5. Verify at http://127.0.0.1:5173 (desktop + narrow viewport).
