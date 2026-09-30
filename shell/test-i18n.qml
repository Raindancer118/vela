// Locale test: env -u LANGUAGE LC_ALL=de_DE.UTF-8 qs -p test-i18n.qml  (also en_US, fr_FR, C); prints PASS or FAIL
import QtQuick
import Quickshell
import qs

ShellRoot {
    Component.onCompleted: {
        const d = new Date(2026, 8, 30, 7, 5);
        const got = [I18n.language, Config.locale.toString(d, Config.timeFormat), Config.locale.toString(d, Config.dateFormat),
                     Config.locale.toString(d, Config.shortDateFormat), I18n.tr("Night light"), I18n.tr("Show %1 more", 3),
                     I18n.relativeTime(30), I18n.relativeTime(300), I18n.relativeTime(3 * 3600), I18n.duration(3900), I18n.uptime(2 * 86400 + 6 * 3600 + 32 * 60), I18n.uptime(32 * 60)];
        const expected = {
            "de_DE": ["de", "07:05", "Mittwoch, 30. September", "30.09.", "Nachtlicht", "3 weitere anzeigen", "gerade eben", "vor 5 Min.", "vor 3 Std.", "1 Std. 5 Min.", "läuft seit 2 T. 6 Std. 32 Min.", "läuft seit 32 Min."],
            "en_US": ["en", "7:05\u202fAM", "Wednesday, September 30", "9/30", "Night light", "Show 3 more", "now", "5 min ago", "3 h ago", "1 h 5 min", "up 2d 6h 32m", "up 32m"],
            "fr_FR": ["fr", "07:05", "mercredi 30 septembre", "30/09", "Night light", "Show 3 more", "now", "5 min ago", "3 h ago", "1 h 5 min", "up 2d 6h 32m", "up 32m"],
            "C":     ["en", "7:05\u202fAM", "Wednesday, September 30", "9/30", "Night light", "Show 3 more", "now", "5 min ago", "3 h ago", "1 h 5 min", "up 2d 6h 32m", "up 32m"],
        }[Quickshell.env("LC_ALL").split(".")[0]] ?? null;
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + Quickshell.env("LC_ALL") + " " + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
