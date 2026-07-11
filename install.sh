#!/usr/bin/env bash
# T-0 — quick install (macOS-first)
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/aatosolavi/t-0/main/install.sh | bash
# Or:
#   ./install.sh
set -euo pipefail

REPO_URL="${MC_REPO_URL:-https://github.com/aatosolavi/t-0.git}"
BRANCH="${MC_BRANCH:-main}"

if [[ -n "${MC_INSTALL_DIR:-}" ]]; then
  INSTALL_DIR="$MC_INSTALL_DIR"
elif [[ -d "${HOME}/dev" ]]; then
  INSTALL_DIR="${HOME}/dev/t-0"
else
  INSTALL_DIR="${HOME}/t-0"
fi

echo "→ T-0 install"
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
  echo "error: rustup is required to build the t0 launcher (https://rustup.rs)" >&2
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

PRODUCT_URL="http://127.0.0.1:4321"
NODE_MAJOR="$(node -p 'process.versions.node.split(".")[0]')"
if [[ "$NODE_MAJOR" -ge 24 ]]; then
  echo "→ Stable URL: https://t0.localhost (portless)"
  echo "  portless may ask for sudo once to trust its local HTTPS CA."
  bunx portless alias t0 4321 || true
  bunx portless proxy start || true
  bunx portless trust || true
  bunx portless service install || true
  if curl -skf --max-time 5 "https://t0.localhost" -o /dev/null 2>/dev/null; then
    PRODUCT_URL="https://t0.localhost"
  else
    echo "  portless setup incomplete — using $PRODUCT_URL (retry later: bunx portless proxy start)"
  fi
else
  echo "→ Skipping stable URL: portless needs Node 24+ (found $NODE_MAJOR). Using $PRODUCT_URL"
fi

echo ""
echo "✓ T-0 is installed"
echo "  Open:  $PRODUCT_URL"
echo "  Logs:  ~/.t-0/logs/"
echo "  Helium: load extension/ as an unpacked extension for Cmd+T → terminal"
echo ""
if command -v open >/dev/null 2>&1; then
  open "$PRODUCT_URL" 2>/dev/null || true
fi
