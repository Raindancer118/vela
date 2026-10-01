pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Settings shared with the vela launcher (~/.config/vela/config.toml),
// streamed as JSON lines by `vela shell-config --watch`: sanitized values
// and a palette derived from the vela theme. Until the first line arrives
// (or without vela) the defaults below, equal to vela's, apply.
Singleton {
    id: root

    property bool loaded: false

    property var colors: ({
            background: "#18181e",
            text: "#ececf4",
            textMuted: "#9696a8",
            textDisabled: "#5d5d6a",
            tint: "#ffffff",
            primary: "#7aa2f7",
            textOnPrimary: "#101014",
            primaryMuted: "#3a486a",
            error: "#f2b8b5",
            textOnError: "#601410",
            errorSurface: "#372e33"
        })
    property var idle: ({
            suspend: true,
            suspendAfterMin: 30
        })
    property var appearance: ({
            light: false,
            radius: 22,
            fontScale: 1,
            opacity: 0.86,
            surfaceOpacity: 0.06,
            animationScale: 1,
            backdropDim: 0.18
        })
    property var panel: ({
            width: 420,
            closeOnFocusLoss: true,
            backdrop: false,
            backdropLayers: 1,
            backdropLayerAlpha: 0.18,
            popupTimeoutMs: 5000,
            popupMaxVisible: 4,
            criticalPopupsStay: true,
            groupCollapsedCount: 2,
            compactNotifications: true,
            workspaceOsd: true,
            nightLightTemperature: 4000,
            clockCentered: false,
            claudeUsage: true,
            claudeUsageSubtle: false
        })

    function apply(line: string): void {
        try {
            const d = JSON.parse(line);
            idle = d.idle;
            colors = d.colors;
            appearance = d.appearance;
            panel = d.panel;
            loaded = true;
        } catch (e) {
            console.warn("VelaConfig: unreadable settings line:", e);
        }
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "shell-config", "--watch"]
        running: true
        stdout: SplitParser {
            onRead: line => root.apply(line)
        }
        stderr: SplitParser {
            onRead: line => console.warn(line)
        }
        onExited: exitCode => {
            console.warn("VelaConfig: vela shell-config exited with", exitCode, "- retrying");
            retry.start();
        }
    }

    Timer {
        id: retry

        interval: 3000
        onTriggered: proc.running = true
    }
}
