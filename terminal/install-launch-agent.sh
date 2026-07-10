#!/usr/bin/env bash
set -euo pipefail

LABEL="com.grok-mission-control.terminal"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
LOG_DIR="$HOME/.grok-mission-control/logs"
LAUNCHER_BIN="$HOME/.grok-mission-control/bin/mc"
BUN_BIN="$(command -v bun)"
NODE_BIN="$(command -v node)"
CODEX_BIN="$(command -v codex || true)"
GROK_BIN="$(command -v grok || true)"
CLAUDE_BIN="$(command -v claude || true)"
AMP_BIN="$(command -v amp || true)"
DEVIN_BIN="$(command -v devin || true)"
DROID_BIN="$(command -v droid || true)"
TERMINAL_PATH="$HOME/.grok-mission-control/bin:$HOME/.npm-global/bin:$HOME/.grok/bin:$HOME/.local/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

mkdir -p "$HOME/Library/LaunchAgents" "$LOG_DIR"

cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$LABEL</string>

  <key>WorkingDirectory</key>
  <string>$ROOT</string>

  <key>ProgramArguments</key>
  <array>
    <string>$NODE_BIN</string>
    <string>$ROOT/terminal/start.mjs</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>BUN_BIN</key>
    <string>$BUN_BIN</string>

    <key>GROK_TERMINAL_LAUNCHER</key>
    <string>$LAUNCHER_BIN</string>

    <key>GROK_TERMINAL_CODEX_COMMAND</key>
    <string>${CODEX_BIN:-codex}</string>

    <key>GROK_TERMINAL_GROK_COMMAND</key>
    <string>${GROK_BIN:-grok}</string>

    <key>GROK_TERMINAL_CLAUDE_COMMAND</key>
    <string>${CLAUDE_BIN:-claude}</string>

    <key>GROK_TERMINAL_AMP_COMMAND</key>
    <string>${AMP_BIN:-amp}</string>

    <key>GROK_TERMINAL_DEVIN_COMMAND</key>
    <string>${DEVIN_BIN:-devin}</string>

    <key>GROK_TERMINAL_DROID_COMMAND</key>
    <string>${DROID_BIN:-droid}</string>

    <key>PATH</key>
    <string>$TERMINAL_PATH</string>
  </dict>

  <key>RunAtLoad</key>
  <true/>

  <key>KeepAlive</key>
  <true/>

  <key>StandardOutPath</key>
  <string>$LOG_DIR/terminal.out.log</string>

  <key>StandardErrorPath</key>
  <string>$LOG_DIR/terminal.err.log</string>
</dict>
</plist>
PLIST

launchctl bootout "gui/$(id -u)" "$PLIST" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl kickstart -k "gui/$(id -u)/$LABEL"

echo "Installed and started $LABEL"
echo "Terminal URL: http://localhost:4321"
echo "Logs:"
echo "  $LOG_DIR/terminal.out.log"
echo "  $LOG_DIR/terminal.err.log"
