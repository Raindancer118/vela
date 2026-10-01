// Updates tile logic: subtitle, highlight and icon for each update state.
//   qs -p test-updates.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const pkgs = n => Array.from({ length: n }, (_, i) => ({ name: "p" + i, source: "repo" }));
        const states = [
            { checkedAt: 0, pending: [] },
            { checkedAt: 5, checking: true, pending: pkgs(2) },
            { checkedAt: 5, pending: pkgs(2), running: "Packages and AUR" },
            { checkedAt: 5, pending: pkgs(2), running: "Packages and AUR", fingerprint: true },
            { checkedAt: 5, pending: pkgs(2), failed: "error: failed to commit transaction" },
            { checkedAt: 5, pending: pkgs(3) },
            { checkedAt: 5, pending: pkgs(1) },
            { checkedAt: 5, pending: [] }
        ];
        const got = states.map(s => [SystemUpdates.subtitle(s), SystemUpdates.highlighted(s), SystemUpdates.icon(s)]);
        const tr = I18n.tr;
        const expected = [
            [tr("Not checked yet"), false, "system_update"],
            [tr("Checking…"), true, "system_update"],
            [tr("Updating…"), true, "system_update"],
            [tr("Touch the fingerprint reader"), true, "fingerprint"],
            [tr("Update failed"), true, "priority_high"],
            [tr("%1 available", 3), true, "system_update"],
            [tr("%1 available", 1), true, "system_update"],
            [tr("Up to date"), false, "system_update"]
        ];
        const pass = JSON.stringify(got) === JSON.stringify(expected);
        console.log((pass ? "PASS " : "FAIL ") + JSON.stringify(got) + (pass ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
