#!/usr/bin/env bash
# Removes a user installation made by install.sh.
#
#   ./uninstall.sh          remove program files; keep settings and history
#   ./uninstall.sh --purge  also delete ~/.config/vela, ~/.local/state/vela
#                           and ~/.cache/vela
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
DATADIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFDIR="${XDG_CONFIG_HOME:-$HOME/.config}"
HYPRDIR="$CONFDIR/hypr"
say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

"$PREFIX/bin/vela" quit 2>/dev/null || true
systemctl --user stop vela.service 2>/dev/null || true

rm -f "$PREFIX/bin/vela" "$PREFIX/bin/vela-daemon" \
    "$DATADIR/applications/vela.desktop" \
    "$DATADIR/icons/hicolor/scalable/apps/vela.svg" \
    "$DATADIR/icons/hicolor/scalable/apps/vela-claude-symbolic.svg" \
    "$DATADIR/icons/hicolor/scalable/apps/vela-drag-handle-symbolic.svg" \
    "$CONFDIR/systemd/user/vela.service" \
    "$HYPRDIR/vela.lua"
rm -rf "$DATADIR/vela/shell"
systemctl --user daemon-reload 2>/dev/null || true

if [[ -f "$HYPRDIR/hyprland.lua" ]] && grep -qF 'hypr/vela.lua' "$HYPRDIR/hyprland.lua"; then
    cp "$HYPRDIR/hyprland.lua" "$HYPRDIR/hyprland.lua.bak-vela-$(date +%s)"
    sed -i '/-- vela launcher: tap Super to open/d; /hypr\/vela\.lua/d' "$HYPRDIR/hyprland.lua"
    say "removed vela from hyprland.lua (backup created)"
fi

if [[ "${1:-}" == "--purge" ]]; then
    rm -rf "$CONFDIR/vela" "${XDG_STATE_HOME:-$HOME/.local/state}/vela" "${XDG_CACHE_HOME:-$HOME/.cache}/vela"
    say "settings, history and cache deleted"
else
    say "settings kept in $CONFDIR/vela (use --purge to delete them)"
fi
say "vela uninstalled"
