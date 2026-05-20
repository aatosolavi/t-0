# Browser Terminal Notes

This branch runs a real local shell inside a browser tab:

- HTML server: `http://localhost:4321`
- PTY broker: `ws://localhost:4322`
- Entry command: `bun run terminal`

The PTY broker is a Node process because `@lydell/node-pty` is more reliable there than under Bun on macOS. The Bun process only serves the HTML and attachment upload endpoint.

## Known Rendering Issue

Codex/Grok TUI input surfaces can show a cursor/background mismatch with a blinking block cursor.

Current understanding:

- xterm renders the block cursor over the actual terminal buffer cell.
- When the cursor blinks off, xterm reveals the cell underneath.
- Some TUIs appear to paint the grey input surface around the cursor but leave the blank cursor cell itself with the default terminal background.
- In that case, the cursor cell looks like it changes background on blink-off, but the browser is showing the real underlying buffer state.

Things that did not hold up:

- A canvas/pixel-sampling overlay was too fragile and broke the renderer model.
- Mixing `xterm` with newer `@xterm/addon-webgl` can blank or break rendering.
- Repositioning `.xterm-screen` directly breaks xterm layout.

Preferred future fixes:

1. Best: fix the TUI output so the input surface paints every cell, including the cursor cell.
2. Browser-side experiment: use xterm buffer metadata/decorations, not canvas sampling, and only patch a cursor cell when it has a clear neighboring background.
3. Acceptable fallback: allow a bar/underline cursor mode for apps that do not paint block cursor cells correctly.

## macOS Login Startup

Use the install script:

```bash
./terminal/install-launch-agent.sh
```

It installs `~/Library/LaunchAgents/com.grok-mission-control.terminal.plist` and starts `bun run terminal` at login. Logs go to:

- `~/.grok-mission-control/logs/terminal.out.log`
- `~/.grok-mission-control/logs/terminal.err.log`

Manual control:

```bash
launchctl kickstart -k gui/$(id -u)/com.grok-mission-control.terminal
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.grok-mission-control.terminal.plist
```

## Repo Picker Direction

The simplest useful version is a small browser-side launcher before attaching to a PTY:

- Keep a JSON list of favorite repo paths.
- Show those repos as buttons when opening the terminal tab.
- Clicking a repo starts a PTY with that repo as cwd.
- Optional buttons choose what to run there: shell, `codex`, `grok`, etc.

That is cleaner than sending `cd ...` into an already-running shell because each browser tab can represent one explicit session: `{cwd, command, agent}`.

For the current MVP, the PTY server already remembers the last cwd. A repo picker should become the next layer above that, not replace it.
