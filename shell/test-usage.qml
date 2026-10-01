// Claude usage section logic: colour level, reset label, account filter.
//   qs -p test-usage.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const now = new Date(2026, 9, 1, 9, 0).getTime(); // Thu 09:00 local
        const at = (d, h, m) => new Date(2026, 9, d, h, m).getTime();
        const got = [
            ClaudeUsage.level(0.1), ClaudeUsage.level(0.5), ClaudeUsage.level(0.79), ClaudeUsage.level(0.8), ClaudeUsage.level(1.2),
            ClaudeUsage.resetLabel(at(1, 14, 0), now) === Config.locale.toString(new Date(at(1, 14, 0)), Config.timeFormat),
            ClaudeUsage.resetLabel(at(3, 10, 0), now).startsWith(Config.locale.toString(new Date(at(3, 10, 0)), "ddd")),
            ClaudeUsage.resetLabel(0, now),
            ClaudeUsage.resetLabel(at(1, 14, 0) - 100, now) === Config.locale.toString(new Date(at(1, 14, 0)), Config.timeFormat),
            ClaudeUsage.select([{ name: "default" }, { name: "a" }, { name: "b" }], false, ["a"]).map(a => a.name),
            ClaudeUsage.select([{ name: "default" }, { name: "a" }], true, []).map(a => a.name),
            // At most 5 minutes old, otherwise no numbers for that account.
            ClaudeUsage.fresh([{ name: "new", fetchedAt: now - 4 * 60000 }, { name: "old", fetchedAt: now - 6 * 60000 }], now).map(a => a.name)
        ];
        const expected = ["low", "mid", "mid", "high", "high", true, true, "", true, ["default", "b"], ["default"], ["new"]];
        const pass = JSON.stringify(got) === JSON.stringify(expected);
        console.log((pass ? "PASS " : "FAIL ") + JSON.stringify(got) + (pass ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
