// Claude notifications: recognised, grouped as "Claude Code", own icon.
//   qs -p test-claude.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const n = (appName, summary, desktopEntry) => ({ appName, summary, desktopEntry: desktopEntry ?? "" });
        const got = [
            Notifications.isClaude(n("", "Claude Code")),
            Notifications.isClaude(n("Claude", "Done")),
            Notifications.isClaude(n("ghostty", "x", "claude-desktop")),
            Notifications.isClaude(n("Discord", "Claude Code fans")),
            Notifications.isClaude(n("", "Screenshot")),
            Notifications.appKey(n("", "Claude Code")),
            Notifications.appKey(n("Firefox", "x")),
            Notifications.appKey(n("", "x")) === I18n.tr("Unknown")
        ];
        const expected = [true, true, true, false, false, "Claude Code", "Firefox", true];
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
