#!/usr/bin/env bash
# Builds vela and installs it for the current user (no root needed).
#
#   ./install.sh              build + install binary, icons, desktop entry,
#                             systemd user unit and ~/.config/hypr/vela.lua
#   ./install.sh --hyprland   additionally add the dofile() line to
#                             ~/.config/hypr/hyprland.lua (a backup is made)
set -euo pipefail

cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
DATADIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFDIR="${XDG_CONFIG_HOME:-$HOME/.config}"
HYPRDIR="$CONFDIR/hypr"
EDIT_HYPR=0
[[ "${1:-}" == "--hyprland" ]] && EDIT_HYPR=1

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*"; }

# Dependencies (Arch package names)
if command -v pacman >/dev/null; then
    missing=()
    for p in gtk4 gtk4-layer-shell libadwaita rust; do
        pacman -Qq "$p" >/dev/null 2>&1 || missing+=("$p")
    done
    if ((${#missing[@]})); then
        warn "missing packages: ${missing[*]}"
        warn "install them with: sudo pacman -S --needed ${missing[*]}"
        exit 1
    fi
    for p in plocate kitty xdg-utils; do
        pacman -Qq "$p" >/dev/null 2>&1 || warn "optional package '$p' is not installed"
    done
fi
command -v claude >/dev/null || [[ -x "$HOME/.local/bin/claude" ]] || warn "Claude Code (claude) not found — the Ask Claude action will report that"

say "building (release)"
cargo build --release --locked

say "installing to $PREFIX"
install -Dm755 target/release/vela "$BINDIR/vela"
install -Dm755 target/release/vela-daemon "$BINDIR/vela-daemon"
for icon in data/icons/*.svg; do
    install -Dm644 "$icon" "$DATADIR/icons/hicolor/scalable/apps/$(basename "$icon")"
done
sed "s|^Exec=vela|Exec=$BINDIR/vela|" data/vela.desktop | install -Dm644 /dev/stdin "$DATADIR/applications/vela.desktop"
sed "s|@BINDIR@|$BINDIR|" contrib/systemd/vela.service.in | install -Dm644 /dev/stdin "$CONFDIR/systemd/user/vela.service"
install -Dm644 contrib/hyprland/vela.lua "$HYPRDIR/vela.lua"
# Overwrite in place (cp keeps the files) so a running shell notices the
# change and reloads itself; drop files that no longer exist.
mkdir -p "$DATADIR/vela/shell"
(cd shell && find . -type f \( -name '*.qml' -o -name '*.svg' \) ! -name 'test-*' ! -path ./shell.qml -exec sh -c 'mkdir -p "$1/$(dirname "$2")" && cp "$2" "$1/$2"' _ "$DATADIR/vela/shell" {} \;)
# shell.qml last, after a pause and always different: the running shell may
# have reloaded mid-copy (old and new files mixed); this separate change makes
# it reload once more with every file in place.
sleep 1
{ cat shell/shell.qml; printf '// installed %s\n' "$(date +%s)"; } >"$DATADIR/vela/shell/shell.qml"
(cd "$DATADIR/vela/shell" && find . -type f \( -name '*.qml' -o -name '*.svg' \) | while read -r f; do [[ -f "$OLDPWD/shell/$f" ]] || rm -f "$f"; done)
command -v qs >/dev/null || warn "Quickshell (qs) not found — the control center (vela shell) needs it"
# Lets Claude Code change Hyprland/vela settings (Settings search → Ask Claude),
# in the default profile and every ccacct profile (~/.claude-accounts/<name>,
# each has its own .claude.json; *.lock are ccacct's leftovers).
if command -v claude >/dev/null; then
    profiles=("")
    for d in "$HOME"/.claude-accounts/*/; do
        [[ -d "$d" && "$d" != *.lock/ && -f "$d.claude.json" ]] && profiles+=("${d%/}")
    done
    for dir in "${profiles[@]}"; do
        name="${dir:+$(basename "$dir")}"
        name="${name:-default}"
        run=(env -u CLAUDE_CONFIG_DIR)
        [[ -n "$dir" ]] && run=(env CLAUDE_CONFIG_DIR="$dir")
        if "${run[@]}" claude mcp get vela >/dev/null 2>&1; then
            say "Claude Code ($name) already knows the vela MCP server"
        elif "${run[@]}" claude mcp add --scope user vela -- "$BINDIR/vela" mcp >/dev/null 2>&1; then
            say "registered the vela MCP server with Claude Code ($name)"
        else
            warn "could not register the MCP server for $name; run: claude mcp add --scope user vela -- $BINDIR/vela mcp"
        fi
    done
fi
systemctl --user daemon-reload 2>/dev/null || true
gtk-update-icon-cache -q -t "$DATADIR/icons/hicolor" 2>/dev/null || true

LINE='dofile(os.getenv("HOME") .. "/.config/hypr/vela.lua").setup()'
if [[ -f "$HYPRDIR/hyprland.lua" ]]; then
    if grep -qF 'hypr/vela.lua' "$HYPRDIR/hyprland.lua"; then
        say "hyprland.lua already loads vela.lua"
    elif ((EDIT_HYPR)); then
        cp "$HYPRDIR/hyprland.lua" "$HYPRDIR/hyprland.lua.bak-vela-$(date +%s)"
        printf '\n-- vela launcher: tap Super to open\n%s\n' "$LINE" >>"$HYPRDIR/hyprland.lua"
        say "added vela to hyprland.lua (backup created)"
    else
        say "add this line to $HYPRDIR/hyprland.lua (or rerun with --hyprland):"
        echo "    $LINE"
    fi
else
    warn "no Lua Hyprland config found; see README → Hyprland for hyprland.conf users"
fi

# Restart a running daemon so the new binary is used.
if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    systemctl --user import-environment WAYLAND_DISPLAY HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_TYPE DISPLAY
    systemctl --user restart vela.service && say "vela daemon (re)started"
elif systemctl --user -q is-active vela.service 2>/dev/null; then
    systemctl --user restart vela.service
fi

case ":$PATH:" in *":$BINDIR:"*) ;; *) warn "$BINDIR is not in PATH" ;; esac
say "done — tap Super in Hyprland, or run: vela"
