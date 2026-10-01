pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Claude plan usage per Claude Code profile. vela-daemon polls in the
// background as often as the endpoint allows (src/claude_usage.rs) and
// writes ~/.cache/vela/claude-usage.json; this only reads that file through
// `vela claude-usage --watch` and never asks the endpoint itself.
Singleton {
    id: root

    // [{ name, plan, status: "ok" | "unavailable", reason?, fetchedAt?, session, week }]
    property var accounts: []
    // Numbers older than this are not current anymore.
    readonly property int maxAge: 300000
    // Logged-in accounts, filtered by the current settings right away; each
    // is shown with numbers if isCurrent(), otherwise as "no current data".
    readonly property var shown: select(listed(accounts), Config.claudeUsageOnlyDefault, Config.claudeUsageHidden)

    // Reasons that mean "logged out", not "temporarily no data".
    readonly property var loggedOut: ["expired", "no-credentials", "bad-credentials", "http-401", "http-403"]
    property real now: Date.now()

    function isCurrent(a: var, nowMs: real): bool {
        return a?.status === "ok" && nowMs - (a.fetchedAt ?? 0) <= root.maxAge;
    }

    function listed(list: var): var {
        return list.filter(a => a.status === "ok" || !root.loggedOut.includes(a.reason));
    }

    function select(list: var, onlyDefault: bool, hidden: var): var {
        return list.filter(a => (!onlyDefault || a.name === "default") && !(hidden ?? []).includes(a.name));
    }

    function level(u: real): string {
        if (u >= 0.8)
            return "high";
        return u >= 0.5 ? "mid" : "low";
    }

    // Time of day if the reset is today, otherwise with the weekday.
    function resetLabel(ms: real, nowMs: real): string {
        if (!(ms > 0))
            return "";
        // The API reports e.g. 10:59:59.9 for an 11:00 reset.
        const d = new Date(Math.round(ms / 60000) * 60000);
        const time = Config.locale.toString(d, Config.timeFormat);
        return d.toDateString() === new Date(nowMs).toDateString() ? time : Config.locale.toString(d, "ddd") + " " + time;
    }

    function apply(line: string): void {
        try {
            const data = JSON.parse(line);
            root.accounts = data.accounts ?? [];
            root.now = Date.now();
        } catch (e) {}
    }

    Timer {
        interval: Config.relativeTimeInterval
        repeat: true
        running: ShellState.panelOpen
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "claude-usage", "--watch"]
        running: Config.claudeUsage
        stdout: SplitParser {
            onRead: line => root.apply(line)
        }
        onExited: retry.start()
    }

    Timer {
        id: retry

        interval: 3000
        onTriggered: proc.running = Config.claudeUsage
    }
}
