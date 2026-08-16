# T-0 website as the pad

Date: 2026-08-16  
Surface: `www/public/index.html` (Vercel `outputDirectory`)

## Goal

The public landing page is the `t0` launcher, not a marketing brochure. A visitor should recognize the product before they install it.

## Surface

- Full-tab terminal: no nav, gradient, pill badge, or rounded marketing buttons.
- Dark: bg `#141414`, text `#e5e5e5`, surface `#262626`, accent `#f97316`.
- Light via `prefers-color-scheme`: bg `#fafafa`, text `#171717`, accent `#ea580c`.
- Font: Geist Mono / ui-monospace, 13px, no ligatures, antialiased.
- Centered rounded panel, title ` T-0 `, max ~92ch, min ~22 rows.
- One live region: the status/tip line. Tips rotate ~30s. No other idle motion.

Panel stack (top → bottom):

1. `Launch` + orange selected-row name + `in a workspace`
2. Agent chips `1 Grok` … `9 Shell` (display / highlight only)
3. Dim `/ type to filter`
4. List
5. Status / tip
6. Key hints

## List

Columns: `▌` · name · meta · kind · path/url.

| section | row | href |
|---|---|---|
| ★ favorites | install (selected on load; `★` prefix) | `#install` |
| recent | github | https://github.com/aatosolavi/t-0 |
| recent | npm | https://www.npmjs.com/package/@aatosolavi/t-0 |
| last | t0.localhost | https://t0.localhost |
| root | readme | GitHub README |

Selected row: full-width surface + orange `▌` + bold name.

Install commands sit **under** the pad (same copy as before). Not a second column.

## Input

- Click row → open. Hover selects.
- ↑↓ / j k (j k only when filter empty) · **enter** opens.
- Filter empty: **i** install, **g** github, **n** npm, **?** help.
- Type filters name/path; **esc** clears (or closes help).
- **1–9** highlight the matching chip only.
- Help overlay: ` T-0 · Keys `, accent border, **esc** / **?** close.
- Open flash on the status line, then back to tips.
- Rows are real `<a>` links (works without JS).

## Out of scope

- Fake PTY / launching agents from the web.
- Changing `terminal/index.html` or the Ratatui binary.
- New build step, webfonts, or JS framework.

## Verify

Serve `www/public`, check dark + light, keyboard, filter, help, `#install` jump, and a narrow viewport.
