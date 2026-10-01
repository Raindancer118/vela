// Settings test: the shell takes its look and behaviour from `vela shell-config`.
//   scripts/shell-test.sh   (sets VELA_BIN and a throwaway config); prints PASS or FAIL
import QtQuick
import Quickshell
import qs


ShellRoot {
    // The throwaway config written by the script.
    readonly property var expected: ({
            primary: "#ff0000",
            text: "#1c1c22",
            radius: 30,
            panelWidth: 500,
            popupTimeout: 7000,
            animationScale: 0.5,
            closeOnFocusLoss: false,
            backdrop: true,
            backdropBlur: false,
            backdropLayers: 1,
            opacity: 0.7,
            bodyFont: Math.round(14 * 1.5),
            clockCentered: true,
            usageSubtle: true,
            compact: false
        })

    function check(): void {
        const got = {
            primary: String(Theme.colors.primary),
            text: String(Theme.colors.text),
            radius: Theme.radius.panel,
            panelWidth: Theme.size.panelWidth,
            popupTimeout: Config.popupTimeout,
            animationScale: Config.animationScale,
            closeOnFocusLoss: Config.closeOnFocusLoss,
            backdrop: Config.panelBackdrop,
            backdropBlur: Config.panelBackdropBlur,
            backdropLayers: Config.panelBackdropLayers,
            opacity: Math.round(Theme.colors.panel.a * 100) / 100,
            bodyFont: Theme.font.body,
            clockCentered: Config.clockCentered,
            usageSubtle: Config.claudeUsageSubtle,
            compact: Config.compactNotifications
        };
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }

    Connections {
        target: VelaConfig

        function onLoadedChanged(): void {
            if (VelaConfig.loaded)
                check();
        }
    }

    Timer {
        interval: 5000
        running: true
        onTriggered: {
            console.log("FAIL settings never arrived from", Quickshell.env("VELA_BIN"));
            Qt.callLater(Qt.quit);
        }
    }
}
