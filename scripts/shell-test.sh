#!/usr/bin/env bash
# Tests of the control center (Quickshell): settings from `vela shell-config`,
# locales, and a start-up of the full shell. Needs `qs` and a Wayland display
# (the current one, or a headless weston like smoke-test.sh).
#
#   scripts/shell-test.sh [path/to/vela]
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="$(realpath "${1:-target/release/vela}")"
tmp="$(mktemp -d)"
weston_pid=""
cleanup() {
    [[ -n "${shell_pid:-}" ]] && kill "$shell_pid" 2>/dev/null || true
    [[ -n "$weston_pid" ]] && kill "$weston_pid" 2>/dev/null || true
    rm -rf "$tmp"
}
trap cleanup EXIT

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$tmp/run}"
    mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
    weston --backend=headless --socket=vela-shell-test --idle-time=0 >"$tmp/weston.log" 2>&1 &
    weston_pid=$!
    export WAYLAND_DISPLAY=vela-shell-test
    for _ in $(seq 50); do [[ -S "$XDG_RUNTIME_DIR/vela-shell-test" ]] && break; sleep 0.1; done
fi

export XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state" XDG_CACHE_HOME="$tmp/cache" VELA_BIN="$BIN"
mkdir -p "$XDG_CONFIG_HOME/vela"
cat >"$XDG_CONFIG_HOME/vela/config.toml" <<'TOML'
[general]
opacity = 0.7
[appearance]
theme = "light"
accent = "#ff0000"
border_radius = 30
font_scale = 1.5
animation_speed = 2.0
[panel]
backdrop_blur = false
width = 500
popup_timeout_secs = 7
close_on_focus_loss = false
backdrop = true
clock_centered = true
clock_on_backdrop = true
clock_font = "DejaVu Serif"
backdrop_clock_size = 200
claude_usage_subtle = true
compact_notifications = false
TOML

fail() { echo "FAIL: $*"; exit 1; }

echo ":: settings"
timeout 20 qs -p shell/test-settings.qml >"$tmp/settings.log" 2>&1 || true
grep -E "PASS|FAIL" "$tmp/settings.log" || { cat "$tmp/settings.log"; fail "settings test gave no result"; }
grep -q PASS "$tmp/settings.log" || fail "settings"

echo ":: audio icons"
out="$(timeout 20 qs -p shell/test-audio.qml 2>&1 | grep -E "PASS|FAIL" || true)"
echo "$out"
[[ "$out" == *PASS* ]] || fail "audio icons"

echo ":: power mode"
out="$(timeout 20 qs -p shell/test-power.qml 2>&1 | grep -E "PASS|FAIL" || true)"
echo "$out"
[[ "$out" == *PASS* ]] || fail "power mode"

for t in highlight details claude usage updates dwell sharepicker media pulse; do
    echo ":: $t"
    out="$(timeout 20 qs -p shell/test-$t.qml 2>&1 | grep -E "PASS|FAIL" || true)"
    echo "$out"
    [[ "$out" == *PASS* ]] || fail "$t"
done

echo ":: share picker through vela (xdph protocol)"
picker="$(dirname "$BIN")/vela-share-picker"
out="$(VELA_SHELL_DIR="$PWD/shell" VELA_SHARE_PREVIEW=screen VELA_SHARE_PREVIEW_SHARE=1 XDPH_WINDOW_SHARING_LIST='1[HC>]kitty[HT>]x[HE>]0[HA>]' \
    timeout 20 "$picker" --allow-token 2>"$tmp/picker.log" || true)"
echo "$out"
[[ "$out" =~ ^\[SELECTION\]r/screen:[^[:space:]]+$ ]] || { cat "$tmp/picker.log"; fail "share picker"; }

# The window itself only headless (weston): locally it would pop up.
if [[ -n "$weston_pid" ]]; then
    echo ":: Pulse window through vela-pulse"
    VELA_SHELL_DIR="$PWD/shell" "$(dirname "$BIN")/vela-pulse" >"$tmp/pulse.log" 2>&1 || { cat "$tmp/pulse.log"; fail "vela-pulse did not start"; }
    for _ in $(seq 100); do pgrep -f "qs -p $PWD/shell/pulse.qml" >/dev/null && break; sleep 0.1; done
    sleep 4
    qslog="$(ls -t "$XDG_RUNTIME_DIR"/quickshell/by-id/*/log.qslog 2>/dev/null | head -1 || true)"
    "$(dirname "$BIN")/vela-pulse" >>"$tmp/pulse.log" 2>&1 || fail "second vela-pulse (raise) failed"
    [[ "$(pgrep -fc "qs -p $PWD/shell/pulse.qml")" == 1 ]] || fail "vela-pulse started a second window"
    pkill -f "qs -p $PWD/shell/pulse.qml" || true
    if [[ -n "$qslog" ]] && qs log "$qslog" 2>/dev/null | grep -E "ERROR.*(scene|qml)|Failed to load|TypeError|ReferenceError|is not defined|Cannot assign"; then
        fail "errors in the Pulse log"
    fi
fi

echo ":: locales"
for l in de_DE en_US fr_FR C; do
    out="$(env -u LANGUAGE LC_ALL=$l.UTF-8 timeout 20 qs -p shell/test-i18n.qml 2>&1 | grep -E "PASS|FAIL" || true)"
    echo "$out"
    [[ "$out" == *PASS* ]] || fail "locale $l"
done

echo ":: start-up of the full shell"
# Own session bus: the notification daemon must not fight the desktop's.
dbus-run-session -- bash -c '"$0" shell >"$1/shell.log" 2>&1 & pid=$!
    for _ in $(seq 100); do grep -q "Configuration Loaded" "$1/shell.log" 2>/dev/null && break; sleep 0.1; done
    sleep 1
    "$0" panel toggle >>"$1/shell.log" 2>&1 && "$0" panel close >>"$1/shell.log" 2>&1; ok=$?
    kill $pid; exit $ok' "$BIN" "$tmp" || { cat "$tmp/shell.log"; fail "ipc to the shell failed"; }
grep -q "Configuration Loaded" "$tmp/shell.log" || { cat "$tmp/shell.log"; fail "shell did not load"; }
# QML/load errors only: missing system services (PipeWire, NetworkManager in CI)
# are reported by their backends and are not the shell's fault.
if grep -E "ERROR.*(scene|qml)|Failed to load|TypeError|ReferenceError|is not defined|Cannot assign" "$tmp/shell.log"; then
    fail "errors in the shell log"
fi
echo "shell tests passed"
