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

# launcher <name> <query>: the launcher as a settings preview (never takes
# the keyboard), on $MON, nearly opaque so little of the desktop shows.
launcher() {
    python3 - "$T/config/vela/config.toml" "$MON" <<'PY'
import re, subprocess, sys, json
path, mon = sys.argv[1], sys.argv[2]
desc = next((m["description"] for m in json.loads(subprocess.check_output(["hyprctl", "monitors", "-j"])) if m["name"] == mon), "")
t = open(path).read()
t = re.sub(r'(?m)^main_monitor = .*$', f'main_monitor = "desc:{desc}"', t)
t = re.sub(r'(?m)^opacity = .*$', 'opacity = 0.94', t, count=1)
t = re.sub(r'(?m)^backdrop = .*$', 'backdrop = false', t)
# Pinned apps with their names: the README shouldn't show placeholder icons.
t = re.sub(r'(?m)^grid = .*$', 'grid = "pinned"', t)
t = re.sub(r'(?m)^show_labels = .*$', 'show_labels = true', t)
open(path, "w").write(t)
PY
    VELA_LAUNCHER_SHOT="$2" XDG_CONFIG_HOME="$T/config" XDG_STATE_HOME="$T/state" setsid ./target/debug/vela-daemon >"$T/log" 2>&1 &
    pid=$!
    sleep 3.2
    local geom
    geom=$(hyprctl layers -j | python3 -c "
import json,sys
for mon in json.load(sys.stdin).values():
    for lv in mon['levels'].values():
        for l in lv:
            if l['namespace'] == 'vela' and l['pid'] == $pid:
                print(f\"{l['x']},{l['y']} {l['w']}x{l['h']}\")")
    grim -g "$geom" "$OUT/$1.png"
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    pid=""
    echo "  $1"
}

# The running control center with four made-up notifications (removed again
# afterwards), the sound detail and the popups. The panel opens on the
# focused monitor; readme-frame.py expects the laptop screen at scale 1.2.
if [[ "${CC:-0}" == 1 ]]; then
    D="$HOME/.local/share/vela/shell"
    layer() {
        hyprctl layers -j | python3 -c "
import json,sys
for m in json.load(sys.stdin).values():
    for lv in m['levels'].values():
        for l in lv:
            if l['namespace']=='$1': print(f\"{l['x']},{l['y']} {l['w']}x{l['h']}\")" | head -1
    }
    apps=(Calendar "Claude Code" Firefox Spotify)
    notify-send -a Calendar -i x-office-calendar "Design review in 10 minutes" "Room 2 · with Ana and Lukas"; sleep 0.3
    notify-send -a "Claude Code" "Claude Code" "Done: all 162 tests pass and the release is tagged."; sleep 0.3
    notify-send -a Firefox -i firefox "Download complete" "vela-0.28.1.tar.gz · 2.4 MB"; sleep 0.3
    notify-send -a Spotify -i spotify "Now playing" "Roxette — It Must Have Been Love"; sleep 1.2
    grim -g "$(layer quickshell-notifications)" "$OUT/cc-popups.png"
    sleep 6
    qs ipc -p "$D" call panel open; sleep 1.5
    geom=$(layer quickshell-panel)
    grim -g "$geom" "$OUT/cc-panel.png"
    qs ipc -p "$D" call panel detail audio; sleep 1.2
    grim -g "$geom" "$OUT/cc-audio.png"
    qs ipc -p "$D" call panel close
    for a in "${apps[@]}"; do qs ipc -p "$D" call notifications dismissApp "$a"; done
    echo "  control center"
    exit 0
fi

if [[ "${LAUNCHER:-0}" == 1 ]]; then
    hyprctl monitors -j | python3 -c "import json,sys;print(next(m['scale'] for m in json.load(sys.stdin) if m['name']=='$MON'))" >"$OUT/scale"
    launcher launcher-grid ""
    launcher launcher-apps "term"
    launcher launcher-claude "Explain RSA to me"
    exit 0
fi

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
