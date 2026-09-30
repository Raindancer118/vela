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

"$BIN" daemon >"$tmp/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 100); do "$BIN" status 2>/dev/null && break; sleep 0.1; done
"$BIN" status || fail "daemon did not come up"
# The socket is bound before GTK starts; wait for the UI side to be ready.
for _ in $(seq 100); do grep -q "found .* applications" "$tmp/daemon.log" 2>/dev/null && break; sleep 0.1; done
[[ -f "$XDG_CONFIG_HOME/vela/config.toml" ]] || fail "default config was not created"

for cmd in show hide toggle toggle "settings" "settings claude" reload show; do
    # shellcheck disable=SC2086
    "$BIN" $cmd || fail "command '$cmd' failed"
    sleep 0.2
    kill -0 "$daemon_pid" 2>/dev/null || fail "daemon died after '$cmd'"
done
grep -q "found .* applications" "$tmp/daemon.log" || fail "application scan did not finish"

"$BIN" quit || fail "quit failed"
for _ in $(seq 50); do kill -0 "$daemon_pid" 2>/dev/null || break; sleep 0.1; done
kill -0 "$daemon_pid" 2>/dev/null && fail "daemon did not exit"
wait "$daemon_pid" || fail "daemon exited with an error"
daemon_pid=""
grep -Eq "panicked|CRITICAL" "$tmp/daemon.log" && fail "errors in daemon log"
echo "smoke test passed"
