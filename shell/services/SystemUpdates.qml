pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Pending system updates. vela-daemon checks and runs updates
// (src/ui/updates.rs) and writes ~/.cache/vela/updates.json; this only
// reads it through `vela updates watch`. Clicking the tile opens
// Settings → Updates, where updates are started.
Singleton {
    id: root

    // { checkedAt, checking, pending: [{ source, name, old, new, id }], checkErrors, running?, failed?, fingerprint }
    property var status: ({ checkedAt: 0, pending: [] })

    function subtitle(s: var): string {
        if (s.running && s.fingerprint)
            return I18n.tr("Touch the fingerprint reader");
        if (s.running)
            return I18n.tr("Updating…");
        if (s.failed)
            return I18n.tr("Update failed");
        if (s.checking)
            return I18n.tr("Checking…");
        if (!(s.checkedAt > 0))
            return I18n.tr("Not checked yet");
        const n = (s.pending ?? []).length;
        return n > 0 ? I18n.tr("%1 available", n) : I18n.tr("Up to date");
    }

    function highlighted(s: var): bool {
        return !!(s.running || s.failed || s.checking) || (s.pending ?? []).length > 0;
    }

    function icon(s: var): string {
        if (s.running && s.fingerprint)
            return "fingerprint";
        return s.failed && !s.running ? "priority_high" : "system_update";
    }

    function openSettings(): void {
        ShellState.closePanel();
        Quickshell.execDetached([Quickshell.env("VELA_BIN") || "vela", "settings", "updates"]);
    }

    function apply(line: string): void {
        try {
            root.status = JSON.parse(line);
        } catch (e) {}
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "updates", "watch"]
        running: Config.updatesTile
        stdout: SplitParser {
            onRead: line => root.apply(line)
        }
        onExited: retry.start()
    }

    Timer {
        id: retry

        interval: 3000
        onTriggered: proc.running = Config.updatesTile
    }
}
