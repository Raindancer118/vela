pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Claude plan usage per Claude Code profile, from `vela claude-usage`
// (unofficial endpoint, see src/claude_usage.rs; the token never reaches
// QML). Fetched when the panel opens and the data is older than
// refreshInterval, and periodically while it stays open; the last result
// is cached so the section shows numbers right away.
Singleton {
    id: root

    readonly property int refreshInterval: 300000
    // Cached numbers of an account older than this are no longer shown.
    readonly property int maxAge: 3600000

    // [{ name, session: { utilization, resetsAt } | null, week: … }]
    property var accounts: []
    // Filtered by the current settings right away (the fetch for a newly
    // shown account follows in the background).
    readonly property var shown: select(accounts, Config.claudeUsageOnlyDefault, Config.claudeUsageHidden)
    property var raw: null
    property real lastAttempt: 0
    property real now: Date.now()
    readonly property bool loading: proc.running

    readonly property string cachePath: (Quickshell.env("XDG_CACHE_HOME") || Quickshell.env("HOME") + "/.cache") + "/vela/claude-usage.json"

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

    // Fresh results win; an account that is temporarily unavailable keeps
    // its cached numbers while they are younger than maxAge. Accounts that
    // are no longer listed (hidden in the settings) disappear.
    function merge(cached: var, fresh: var, nowMs: real): var {
        const old = {};
        for (const a of cached?.accounts ?? [])
            if (a.status === "ok")
                old[a.name] = { account: a, at: a.fetchedAt ?? cached.fetchedAt };
        return (fresh?.accounts ?? []).map(a => {
            if (a.status === "ok")
                return Object.assign({ fetchedAt: fresh.fetchedAt }, a);
            const o = old[a.name];
            return o && nowMs - o.at < root.maxAge ? Object.assign({ fetchedAt: o.at }, o.account) : null;
        }).filter(a => a !== null);
    }

    function refreshIfStale(): void {
        now = Date.now();
        if (now - lastAttempt >= refreshInterval)
            refresh();
    }

    function refresh(): void {
        if (proc.running || !Config.claudeUsage)
            return;
        lastAttempt = Date.now();
        proc.running = true;
    }

    Connections {
        target: ShellState

        function onPanelOpenChanged(): void {
            if (!ShellState.panelOpen)
                return;
            root.refreshIfStale();
            // Cheap check (no request) for accounts added, logged in or hidden.
            if (!proc.running)
                listProc.running = true;
        }
    }

    // Accounts hidden or shown in the settings: fetch the new selection.
    Connections {
        target: VelaConfig

        function onPanelChanged(): void {
            if (root.raw !== null) {
                root.lastAttempt = 0;
                root.refresh();
            }
        }
    }

    Timer {
        interval: root.refreshInterval
        repeat: true
        running: ShellState.panelOpen && Config.claudeUsage
        onTriggered: root.refresh()
    }

    Timer {
        interval: Config.relativeTimeInterval
        repeat: true
        running: ShellState.panelOpen
        onTriggered: root.now = Date.now()
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "claude-usage"]
        stdout: StdioCollector {
            onStreamFinished: {
                let fresh = null;
                try {
                    fresh = JSON.parse(text);
                } catch (e) {
                    return;
                }
                const merged = root.merge(root.raw, fresh, Date.now());
                root.accounts = merged;
                root.raw = { fetchedAt: fresh.fetchedAt, accounts: merged };
                cache.setText(JSON.stringify(root.raw));
            }
        }
    }

    Process {
        id: listProc

        command: [Quickshell.env("VELA_BIN") || "vela", "claude-usage", "--list"]
        stdout: StdioCollector {
            onStreamFinished: {
                let names = null;
                try {
                    names = JSON.parse(text);
                } catch (e) {
                    return;
                }
                // Only new names count: expired accounts stay listed from the
                // cache for a while and must not cause a request every time.
                const shown = root.accounts.map(a => a.name);
                if (names.some(n => !shown.includes(n))) {
                    root.lastAttempt = 0;
                    root.refresh();
                }
            }
        }
    }

    FileView {
        id: cache

        path: root.cachePath
        printErrors: false
        onLoaded: {
            try {
                const c = JSON.parse(text());
                root.raw = c;
                root.accounts = root.merge(c, { fetchedAt: c.fetchedAt, accounts: c.accounts.map(a => ({ name: a.name, status: "unavailable" })) }, Date.now());
            } catch (e) {}
        }
    }
}
