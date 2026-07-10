#!/usr/bin/env bash
# Launchpad (mission-control) — quick install (macOS-first)
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/aatosolavi/mission-control/main/install.sh | bash
# Or:
#   ./install.sh
set -euo pipefail

REPO_URL="${MC_REPO_URL:-https://github.com/aatosolavi/mission-control.git}"
BRANCH="${MC_BRANCH:-main}"

if [[ -n "${MC_INSTALL_DIR:-}" ]]; then
  INSTALL_DIR="$MC_INSTALL_DIR"
elif [[ -d "${HOME}/dev" ]]; then
  INSTALL_DIR="${HOME}/dev/mission-control"
else
  INSTALL_DIR="${HOME}/mission-control"
fi

echo "→ Launchpad install"
echo "  dir: $INSTALL_DIR"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing required command: $1" >&2
    exit 1
  fi
}

need git
need node
need bun

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup is required to build the mc launcher (https://rustup.rs)" >&2
  exit 1
fi

if [[ -d "$INSTALL_DIR/.git" ]]; then
  echo "→ Updating existing clone"
  git -C "$INSTALL_DIR" fetch origin
  git -C "$INSTALL_DIR" checkout "$BRANCH"
  git -C "$INSTALL_DIR" pull --ff-only origin "$BRANCH" || true
else
  echo "→ Cloning"
  mkdir -p "$(dirname "$INSTALL_DIR")"
  git clone --branch "$BRANCH" "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"
echo "→ bun install"
bun install

echo "→ Build launcher + LaunchAgent"
bun run terminal:install

echo ""
echo "✓ Launchpad is installed"
echo "  Open:  http://127.0.0.1:4321"
echo "  Logs:  ~/.mission-control/logs/ (or ~/.grok-mission-control/logs if you still use the legacy data dir)"
echo "  Helium: load extension/ as an unpacked extension for Cmd+T → terminal"
echo ""
if command -v open >/dev/null 2>&1; then
  open "http://127.0.0.1:4321" 2>/dev/null || true
fi
