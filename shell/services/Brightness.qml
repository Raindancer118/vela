pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Backlight via brightnessctl. The sysfs value does not emit change events,
// so it is re-read when the panel opens instead of polling.
Singleton {
    id: root

    property bool available: false
    property int current: 0
    property int max: 1
    readonly property real value: max > 0 ? current / max : 0

    // Latest requested raw value; applied by setProc, coalescing fast drags.
    property int pending: -1

    function refresh(): void {
        if (!infoProc.running)
            infoProc.running = true;
    }

    function set(v: real): void {
        if (!available)
            return;
        // Never go fully dark from the slider.
        const raw = Math.round(Math.max(0.01, Math.min(1, v)) * max);
        current = raw;
        pending = raw;
        applyPending();
    }

    function applyPending(): void {
        if (setProc.running || pending < 0)
            return;
        setProc.command = ["brightnessctl", "--class=backlight", "--quiet", "set", String(pending)];
        pending = -1;
        setProc.running = true;
    }

    Component.onCompleted: refresh()

    Process {
        id: infoProc

        // Machine readable: device,class,current,percent,max
        command: ["brightnessctl", "--class=backlight", "--machine-readable", "info"]
        stdout: StdioCollector {
            onStreamFinished: {
                const fields = text.trim().split(",");
                if (fields.length >= 5) {
                    root.current = parseInt(fields[2]);
                    root.max = Math.max(1, parseInt(fields[4]));
                    root.available = true;
                }
            }
        }
        onExited: exitCode => {
            // Exit code 1 means there is no backlight device (desktop).
            if (exitCode !== 0)
                root.available = false;
        }
    }

    Process {
        id: setProc

        onExited: root.applyPending()
    }
}
