// Power mode tile logic: cycle order, icons, labels, powerprofilesctl output.
//   qs -p test-power.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const all = ["power-saver", "balanced", "performance"];
        const two = ["power-saver", "balanced"];
        const list = "* performance:\n    CpuDriver:\tintel_pstate\n    Degraded:   no\n\n  balanced:\n    CpuDriver:\tintel_pstate\n\n  power-saver:\n    CpuDriver:\tintel_pstate\n";
        const got = [
            PowerMode.next("power-saver", all), PowerMode.next("balanced", all), PowerMode.next("performance", all),
            PowerMode.next("balanced", two), PowerMode.next("power-saver", two), PowerMode.next("unknown", all),
            PowerMode.icon("power-saver"), PowerMode.icon("balanced"), PowerMode.icon("performance"),
            PowerMode.label("power-saver"), PowerMode.label("balanced"), PowerMode.label("performance"),
            PowerMode.parseList(list)
        ];
        const expected = ["balanced", "performance", "power-saver", "power-saver", "balanced", "balanced",
            "energy_savings_leaf", "balance", "speed",
            I18n.tr("Power saver"), I18n.tr("Balanced"), I18n.tr("Performance"),
            ["power-saver", "balanced", "performance"]];
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
