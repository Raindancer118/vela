// Claude usage section logic: colour level, reset label, merging with the cache.
//   qs -p test-usage.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const now = new Date(2026, 9, 1, 9, 0).getTime(); // Thu 09:00 local
        const at = (d, h, m) => new Date(2026, 9, d, h, m).getTime();
        const ok = (name, s, w) => ({ name, status: "ok", session: s, week: w });
        const win = u => ({ utilization: u, resetsAt: at(1, 14, 0) });
        const old = { fetchedAt: now - 10 * 60000, accounts: [ok("a", win(0.2), win(0.3)), ok("b", win(0.9), win(0.5))] };
        const fresh = { fetchedAt: now, accounts: [ok("a", win(0.25), win(0.3)), { name: "b", status: "unavailable", reason: "expired" }, { name: "c", status: "unavailable", reason: "expired" }] };
        const merged = ClaudeUsage.merge(old, fresh, now);
        const got = [
            ClaudeUsage.level(0.1), ClaudeUsage.level(0.5), ClaudeUsage.level(0.79), ClaudeUsage.level(0.8), ClaudeUsage.level(1.2),
            ClaudeUsage.resetLabel(at(1, 14, 0), now) === Config.locale.toString(new Date(at(1, 14, 0)), Config.timeFormat),
            ClaudeUsage.resetLabel(at(3, 10, 0), now).startsWith(Config.locale.toString(new Date(at(3, 10, 0)), "ddd")),
            ClaudeUsage.resetLabel(0, now),
            ClaudeUsage.resetLabel(at(1, 14, 0) - 100, now) === Config.locale.toString(new Date(at(1, 14, 0)), Config.timeFormat),
            merged.map(a => a.name + ":" + a.session.utilization),
            ClaudeUsage.merge(old, fresh, now + 2 * 3600000).map(a => a.name),
            ClaudeUsage.select([{ name: "default" }, { name: "a" }, { name: "b" }], false, ["a"]).map(a => a.name),
            ClaudeUsage.select([{ name: "default" }, { name: "a" }], true, []).map(a => a.name),
            // Known = in the last answer, even if unavailable (e.g. HTTP 429).
            ClaudeUsage.hasNew(["default", "a"], { accounts: [{ name: "default", status: "ok" }, { name: "a", status: "unavailable" }] }),
            ClaudeUsage.hasNew(["default", "b"], { accounts: [{ name: "default", status: "ok" }] }),
            ClaudeUsage.hasNew(["default"], null)
        ];
        const expected = ["low", "mid", "mid", "high", "high", true, true, "", true, ["a:0.25", "b:0.9"], ["a"], ["default", "b"], ["default"], false, true, true];
        const pass = JSON.stringify(got) === JSON.stringify(expected);
        console.log((pass ? "PASS " : "FAIL ") + JSON.stringify(got) + (pass ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
