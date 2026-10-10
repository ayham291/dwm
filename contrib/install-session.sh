#!/usr/bin/env bash
# Install dwm-wayland binary + display-manager session entry.
# Run from the repo root after: cargo build --release -p dwm-wayland
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT}/target/release/dwm-wayland"
DESKTOP="${ROOT}/contrib/dwm-wayland.desktop"

if [[ ! -x "$BIN" ]]; then
  echo "missing $BIN — run: cargo build --release -p dwm-wayland" >&2
  exit 1
fi

install -Dm755 "$BIN" /usr/local/bin/dwm-wayland
install -Dm644 "$DESKTOP" /usr/local/share/wayland-sessions/dwm-wayland.desktop
echo "Installed /usr/local/bin/dwm-wayland and wayland-sessions entry."
echo "Log out and pick 'dwm-wayland' in your display manager (or start from a TTY)."
