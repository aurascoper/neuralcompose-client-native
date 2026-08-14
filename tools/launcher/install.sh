#!/usr/bin/env bash
# Install (or refresh) the desktop entry. Idempotent — run it again after moving
# the checkout and the Exec= path is rewritten.
set -euo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/neuralcompose-session"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
DEST="$APPS/neuralcompose.desktop"

[ -f "$SCRIPT" ] || { echo "missing $SCRIPT" >&2; exit 1; }
chmod +x "$SCRIPT"

# Terminal=true is resolved by xdg-terminal-exec on most desktops. Without it
# some environments silently run the entry with no terminal attached, which for
# this app means an interactive session with nowhere to type.
if ! command -v xdg-terminal-exec >/dev/null 2>&1; then
    echo "note: xdg-terminal-exec is not installed." >&2
    echo "      Terminal=true may not open a window on this desktop." >&2
fi

mkdir -p "$APPS"
sed "s|@SCRIPT@|$SCRIPT|g" "$HERE/neuralcompose.desktop" > "$DEST"
chmod 644 "$DEST"

command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$APPS" 2>/dev/null || true

echo "installed $DEST"
echo "  Exec = $SCRIPT"
echo
echo "Config (optional): ${XDG_CONFIG_HOME:-$HOME/.config}/neuralcompose/launcher.conf"
echo "Uninstall:         rm $DEST"
