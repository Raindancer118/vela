#!/usr/bin/env bash
# Start-up test: runs the daemon against a real Wayland display (the current
# one, or a headless weston if none is available) with a throwaway config and
# drives it through its IPC commands.
#
#   scripts/smoke-test.sh [path/to/vela]
set -euo pipefail

BIN="$(realpath "${1:-target/release/vela}")"
tmp="$(mktemp -d)"
export XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state" XDG_CACHE_HOME="$tmp/cache"
export VELA_SOCKET="$tmp/vela.sock" RUST_LOG=vela=info
# The real home is shared: don't poll the real Claude accounts.
export VELA_NO_CLAUDE_USAGE=1
# The settings search borrows rows from every page; this runs it once.
export VELA_SETTINGS_SEARCH="blur gaps" VELA_SETTINGS_DIALOG=shortcut
# Settings → Updates runs "update everything" against fake package tools.
mkdir -p "$tmp/bin"
printf '#!/bin/sh\necho "zlib 1-1 -> 1-2"\n' >"$tmp/bin/checkupdates"
printf '#!/bin/sh\n[ "$1" = -A ] && shift\nexec "$@"\n' >"$tmp/bin/sudo"
printf '#!/bin/sh\necho "fake pacman $*"\n' >"$tmp/bin/pacman"
printf '#!/bin/sh\n' >"$tmp/bin/askpass"
chmod +x "$tmp/bin/"*
export PATH="$tmp/bin:$PATH" VELA_UPDATES_RUN=full SUDO_ASKPASS="$tmp/bin/askpass"
weston_pid=""
cleanup() {
    [[ -n "${daemon_pid:-}" ]] && kill "$daemon_pid" 2>/dev/null || true
    [[ -n "$weston_pid" ]] && kill "$weston_pid" 2>/dev/null || true
    rm -rf "$tmp"
}
trap cleanup EXIT

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$tmp/run}"
    mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
    weston --backend=headless --socket=vela-smoke --idle-time=0 >"$tmp/weston.log" 2>&1 &
    weston_pid=$!
    export WAYLAND_DISPLAY=vela-smoke
    for _ in $(seq 50); do [[ -S "$XDG_RUNTIME_DIR/vela-smoke" ]] && break; sleep 0.1; done
fi

fail() { echo "FAIL: $*"; echo "--- daemon log"; cat "$tmp/daemon.log"; exit 1; }

# Applications reached through a Nix-style profile link (current -> profile ->
# profile-1-link -> store/gen-1); a rebuild only repoints the links.
desktop() { printf '[Desktop Entry]\nType=Application\nName=%s\nExec=true\n' "$1" >"$tmp/store/$2/share/applications/$1.desktop"; }
mkdir -p "$tmp/store/gen-1/share/applications" "$tmp/store/gen-2/share/applications" "$tmp/profiles"
desktop VelaSmokeOld gen-1 && desktop VelaSmokeOld gen-2 && desktop VelaSmokeNew gen-2
ln -s ../store/gen-1 "$tmp/profiles/profile-1-link" && ln -s profile-1-link "$tmp/profiles/profile"
ln -s "$tmp/profiles/profile" "$tmp/current"
export XDG_DATA_DIRS="$tmp/current/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

"$BIN" daemon >"$tmp/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 100); do "$BIN" status 2>/dev/null && break; sleep 0.1; done
"$BIN" status || fail "daemon did not come up"
# The socket is bound before GTK starts; wait for the UI side to be ready.
for _ in $(seq 100); do grep -q "found .* applications" "$tmp/daemon.log" 2>/dev/null && break; sleep 0.1; done
[[ -f "$XDG_CONFIG_HOME/vela/config.toml" ]] || fail "default config was not created"
"$BIN" set updates.aur false && "$BIN" set updates.flatpak false || fail "vela set"

for cmd in show hide toggle toggle "settings" "settings claude" "settings home" "settings hypr-windows" "settings hypr-effects" "settings hypr-animations" "settings hypr-input" "settings hypr-monitors" "settings hypr-shortcuts" "settings hypr-rules" "settings hypr-autostart" "settings hypr-layouts" "settings hypr-behaviour" "settings hypr-all" "settings updates" "updates check" reload show; do
    # shellcheck disable=SC2086
    "$BIN" $cmd || fail "command '$cmd' failed"
    sleep 0.2
    kill -0 "$daemon_pid" 2>/dev/null || fail "daemon died after '$cmd'"
done
grep -q "found .* applications" "$tmp/daemon.log" || fail "application scan did not finish"

# Profile switch: repoint the generation link atomically, like nix-env/nixos-rebuild.
apps() { grep -o "found [0-9]* applications" "$tmp/daemon.log" | tail -1 | grep -o "[0-9]*"; }
sleep 1
before=$(apps)
ln -s ../store/gen-2 "$tmp/profiles/profile-2-link"
ln -s profile-2-link "$tmp/profiles/profile.tmp" && mv -T "$tmp/profiles/profile.tmp" "$tmp/profiles/profile"
for _ in $(seq 50); do [[ "$(apps)" == $((before + 1)) ]] && break; sleep 0.1; done
[[ "$(apps)" == $((before + 1)) ]] || fail "profile switch not noticed ($before -> $(apps) applications)"
# And again after the switch: the watches must follow the new generation.
desktop VelaSmokeNewer gen-2
for _ in $(seq 50); do [[ "$(apps)" == $((before + 2)) ]] && break; sleep 0.1; done
[[ "$(apps)" == $((before + 2)) ]] || fail "new generation not watched ($before -> $(apps) applications)"

status="$XDG_CACHE_HOME/vela/updates.json"
for _ in $(seq 100); do grep -qs '"running":null' "$status" && grep -qs "fake pacman" "$XDG_STATE_HOME"/vela/updates/*.log && break; sleep 0.1; done
grep -qs -- "fake pacman -Syu --noconfirm" "$XDG_STATE_HOME"/vela/updates/*.log || fail "update run did not happen"
grep -qs '"failed":null' "$status" || fail "update run failed: $(cat "$status")"
"$BIN" updates list | grep -q '"zlib"' || fail "vela updates list"

"$BIN" quit || fail "quit failed"
for _ in $(seq 50); do kill -0 "$daemon_pid" 2>/dev/null || break; sleep 0.1; done
kill -0 "$daemon_pid" 2>/dev/null && fail "daemon did not exit"
wait "$daemon_pid" || fail "daemon exited with an error"
daemon_pid=""
grep -Eq "panicked|CRITICAL" "$tmp/daemon.log" && fail "errors in daemon log"

# A selective install (install.sh --profile panel): pages of missing parts are
# left out, `vela` opens the settings instead of the launcher.
printf 'profile = "panel"\nlauncher = false\nclaude = false\nhyprland = false\n' >"$tmp/components.toml"
export VELA_COMPONENTS="$tmp/components.toml"
"$BIN" components | grep -q '\[ \] launcher' || fail "vela components ignores the selection"
unset VELA_SETTINGS_SEARCH VELA_SETTINGS_DIALOG VELA_UPDATES_RUN
VELA_COMPONENTS="$tmp/components.toml" "$BIN" daemon >"$tmp/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 100); do "$BIN" status 2>/dev/null && break; sleep 0.1; done
for cmd in toggle show "settings" "settings panel" "settings system" "settings claude" "settings hypr-windows" hide; do
    # shellcheck disable=SC2086
    "$BIN" $cmd || fail "command '$cmd' failed (panel profile)"
    sleep 0.2
    kill -0 "$daemon_pid" 2>/dev/null || fail "daemon died after '$cmd' (panel profile)"
done
"$BIN" quit || fail "quit failed (panel profile)"
wait "$daemon_pid" || fail "daemon exited with an error (panel profile)"
daemon_pid=""
grep -Eq "panicked|CRITICAL" "$tmp/daemon.log" && fail "errors in daemon log (panel profile)"
echo "smoke test passed"
