pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Night light: runs hyprsunset while enabled. Stopping the process restores
// the normal color temperature. If hyprsunset dies it is restarted a few
// times; after that the toggle turns itself off so the UI matches reality.
Singleton {
    id: root

    readonly property bool enabled: Persist.nightLight
    readonly property int maxRestarts: 3
    property int restarts: 0

    function toggle(): void {
        Persist.nightLight = !Persist.nightLight;
    }

    onEnabledChanged: {
        restarts = 0;
        proc.running = enabled;
    }

    Component.onCompleted: proc.running = enabled

    Process {
        id: proc

        command: ["hyprsunset", "--temperature", String(Config.nightLightTemperature)]
        onExited: exitCode => {
            if (!root.enabled)
                return;
            if (root.restarts < root.maxRestarts) {
                root.restarts++;
                console.warn("NightLight: hyprsunset exited with code", exitCode, "- restarting");
                restartTimer.start();
            } else {
                console.warn("NightLight: hyprsunset keeps exiting, turning night light off");
                Persist.nightLight = false;
            }
        }
    }

    Timer {
        id: restartTimer

        interval: 1000
        onTriggered: proc.running = root.enabled
    }
}
