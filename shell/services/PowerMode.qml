pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Power profile of power-profiles-daemon; the tile cycles power saver →
// balanced → performance. Uses powerprofilesctl: Quickshell's UPower
// PowerProfiles (0.3.1) reports "balanced" without a performance profile
// on machines that have one, while setting works.
Singleton {
    id: root

    // "power-saver", "balanced", "performance"; "" = daemon not available.
    property string profile: ""
    property var profiles: []
    readonly property bool available: profile !== ""

    readonly property var order: ["power-saver", "balanced", "performance"]

    function next(current: string, available: var): string {
        const usable = order.filter(p => available.includes(p));
        const i = usable.indexOf(current);
        return i < 0 ? "balanced" : usable[(i + 1) % usable.length];
    }

    function icon(p: string): string {
        if (p === "power-saver")
            return "energy_savings_leaf";
        return p === "performance" ? "speed" : "balance";
    }

    function label(p: string): string {
        if (p === "power-saver")
            return I18n.tr("Power saver");
        return p === "performance" ? I18n.tr("Performance") : I18n.tr("Balanced");
    }

    function description(p: string): string {
        if (p === "power-saver")
            return I18n.tr("Longer battery life, less performance");
        return p === "performance" ? I18n.tr("Full performance, higher power use") : I18n.tr("Normal performance and power use");
    }

    // Profile names from `powerprofilesctl list`, in `order`.
    function parseList(text: string): var {
        const found = text.split("\n").map(l => l.match(/^\*?\s*([a-z-]+):$/)).filter(m => m).map(m => m[1]);
        return order.filter(p => found.includes(p));
    }

    function refresh(): void {
        getProc.running = true;
        listProc.running = true;
    }

    function set(target: string): void {
        if (!available || !profiles.includes(target) || target === profile)
            return;
        profile = target;
        setProc.command = ["powerprofilesctl", "set", target];
        setProc.running = true;
    }

    function cycle(): void {
        set(next(profile, profiles));
    }

    Component.onCompleted: refresh()

    // Other tools may change it; the panel shows it fresh when opened.
    Connections {
        target: ShellState

        function onPanelOpenChanged(): void {
            if (ShellState.panelOpen)
                root.refresh();
        }
    }

    Process {
        id: getProc

        command: ["powerprofilesctl", "get"]
        stdout: StdioCollector {
            onStreamFinished: root.profile = root.order.includes(text.trim()) ? text.trim() : ""
        }
    }

    Process {
        id: listProc

        command: ["powerprofilesctl", "list"]
        stdout: StdioCollector {
            onStreamFinished: root.profiles = root.parseList(text)
        }
    }

    Process {
        id: setProc

        onExited: root.refresh()
    }
}
