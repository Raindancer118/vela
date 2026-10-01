#!/usr/bin/env bash
# Screenshots of the settings window for the README, taken from a throwaway
# vela (own config/state, own socket) inside the running Hyprland. The window
# never takes the focus, so this can run while you work.
#
#   scripts/readme-shots.sh [monitor]      → /tmp/vela-shots/*.png
set -euo pipefail
cd "$(dirname "$0")/.."
MON="${1:-eDP-1}"
OUT=/tmp/vela-shots
T="$(mktemp -d)"
mkdir -p "$OUT" "$T/config/vela" "$T/state" "$T/run"
cp "${XDG_CONFIG_HOME:-$HOME/.config}/vela/config.toml" "$T/config/vela/" 2>/dev/null || true
cat >"$T/config/vela/hyprland.toml" <<'EOF'
[options]

[[shortcuts]]
keys = "SUPER + B"
action = "exec"
arg = "firefox --new-window"
enabled = true

[[shortcuts]]
keys = "SUPER + SHIFT + F"
action = "fullscreen"
enabled = true

[[shortcuts]]
keys = "SUPER + G"
action = "special"
arg = "scratch"
description = "Scratchpad"
enabled = true

[[rules]]
name = "pavucontrol"
class = "^org\\.pulseaudio\\.pavucontrol$"
title = ""
enabled = true
[rules.effects]
float = true
center = true
size = "900 600"

[[rules]]
name = "picture-in-picture"
class = "^firefox$"
title = "^Picture-in-Picture$"
enabled = true
[rules.effects]
float = true
pin = true
size = "640 360"
opacity = 0.95

[[autostart]]
command = "nm-applet --indicator"
enabled = true
EOF
export VELA_SOCKET="$T/run/vela.sock"
cargo build -q

hyprctl eval "_G.velaShots = hl.window_rule({ name = \"vela-shots\", match = { class = \"^io.github.raindancer118.Vela\$\" }, no_focus = true, float = true, size = \"1060 840\", monitor = \"$MON\", center = true })" >/dev/null
cleanup() {
    [[ -n "${pid:-}" ]] && kill "$pid" 2>/dev/null || true
    hyprctl eval '_G.velaShots:set_enabled(false)' >/dev/null || true
    rm -rf "$T"
}
trap cleanup EXIT

# shot <name> <page> [search] [dialog]
shot() {
    VELA_SETTINGS_SEARCH="${3:-}" VELA_SETTINGS_DIALOG="${4:-}" XDG_CONFIG_HOME="$T/config" XDG_STATE_HOME="$T/state" \
        setsid ./target/debug/vela-daemon >"$T/log" 2>&1 &
    pid=$!
    for _ in $(seq 50); do ./target/debug/vela status 2>/dev/null && break; sleep 0.1; done
    sleep 0.8
    ./target/debug/vela settings "$2"
    sleep "${WAIT:-1.8}"
    local geom
    geom=$(hyprctl clients -j | python3 -c "import json,sys;[print(f\"{c['at'][0]},{c['at'][1]} {c['size'][0]}x{c['size'][1]}\") for c in json.load(sys.stdin) if c['pid']==$pid]")
    grim -g "$geom" "$OUT/$1.png"
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    pid=""
    echo "  $1"
}

if [[ "${DEMO:-0}" == 1 ]]; then
    # Frames of someone typing into the settings search.
    rm -f "$OUT"/demo-*.png
    i=0
    for q in "" "A" "Abst" "Abstand" "Abstand zw" "Abstand zwischen" "Abstand zwischen Fen" "Abstand zwischen Fenstern" \
        "" "b" "bl" "blu" "blur"; do
        WAIT=2.4 shot "$(printf 'demo-%02d' $i)" home "$q"
        i=$((i + 1))
    done
    exit 0
fi

shot home home
shot search-de home "Abstand zwischen Fenstern"
shot search-blur home "blur"
shot claude home "Make the borders thinner and blur stronger"
shot windows hypr-windows
shot effects hypr-effects
shot animations hypr-animations
shot monitors hypr-monitors
shot shortcuts hypr-shortcuts
shot shortcut-dialog hypr-shortcuts "" shortcut
shot rules hypr-rules
shot rule-dialog hypr-rules "" rule
shot all hypr-all
shot appearance appearance
echo "→ $OUT"
