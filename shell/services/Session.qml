pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import qs

// Session actions for the power menu, plus uptime.
Singleton {
    id: root

    // Boot time is read once from /proc/uptime; the uptime label is then
    // derived from the clock, which only ticks while the panel is open.
    property real bootTime: 0
    readonly property string uptime: {
        if (bootTime <= 0)
            return "";
        return I18n.uptime(Math.max(0, Math.floor((clock.date.getTime() - bootTime) / 1000)));
    }

    // vela's settings window (launcher, control center, appearance).
    function openSettings(): void {
        ShellState.closePanel();
        Quickshell.execDetached([Quickshell.env("VELA_BIN") || "vela", "settings", "panel"]);
    }

    function lock(): void {
        ShellState.closePanel();
        Quickshell.execDetached(Config.lockCommand);
    }

    function logout(): void {
        Hyprland.dispatch("exit");
    }

    function reboot(): void {
        Quickshell.execDetached(Config.rebootCommand);
    }

    function poweroff(): void {
        Quickshell.execDetached(Config.poweroffCommand);
    }

    SystemClock {
        id: clock

        enabled: ShellState.panelOpen
        precision: SystemClock.Minutes
    }

    FileView {
        path: "/proc/uptime"
        onLoaded: root.bootTime = Date.now() - parseFloat(text().split(" ")[0]) * 1000
    }
}
