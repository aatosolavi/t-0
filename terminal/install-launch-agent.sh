#!/usr/bin/env bash
set -euo pipefail

LABEL="com.grok-mission-control.terminal"
ROOT="/Users/aatosmononen/Documents/10-19 Work/Personal Projects/grok-mission-control"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
LOG_DIR="$HOME/.grok-mission-control/logs"
BUN_BIN="$(command -v bun)"

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
    <string>$BUN_BIN</string>
    <string>run</string>
    <string>terminal</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>BUN_BIN</key>
    <string>$BUN_BIN</string>

    <key>PATH</key>
    <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
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
