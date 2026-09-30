// Power mode tile logic: cycle order, icons and labels.
//   qs -p test-power.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import Quickshell.Services.UPower
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const S = PowerProfile.PowerSaver, B = PowerProfile.Balanced, P = PowerProfile.Performance;
        const got = [
            PowerMode.next(S, true) === B,
            PowerMode.next(B, true) === P,
            PowerMode.next(P, true) === S,
            PowerMode.next(B, false) === S, "without performance: saver <-> balanced",
            PowerMode.next(S, false) === B,
            PowerMode.icon(S), PowerMode.icon(B), PowerMode.icon(P),
            PowerMode.label(S), PowerMode.label(B), PowerMode.label(P)
        ];
        const expected = [true, true, true, true, "without performance: saver <-> balanced", true,
            "energy_savings_leaf", "balance", "speed", I18n.tr("Power saver"), I18n.tr("Balanced"), I18n.tr("Performance")];
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
