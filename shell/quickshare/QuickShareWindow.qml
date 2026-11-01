import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import qs
import qs.components
import qs.services
import qs.details

// Quick Share in a window of its own: the same view as in the panel, for
// the launcher entry (`vela quickshare open`) and the panel's pop-out button.
FloatingWindow {
    id: win

    title: "Quick Share"
    implicitWidth: 460
    implicitHeight: 680
    minimumSize: Qt.size(380, 520)
    color: Theme.colors.panel
    // Closed by the compositor (its X, Super+Q): forget it.
    onVisibleChanged: if (!visible)
        ShellState.quickShareWindow = false

    function raise(): void {
        Hyprland.dispatch(`hl.dsp.focus({ window = "title:^${title}$" })`);
    }

    Connections {
        target: ShellState

        function onRaiseQuickShare(): void {
            win.raise();
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: ShellState.quickShareWindow = false
    }

    QuickShareDetail {
        anchors.fill: parent
        anchors.margins: Theme.spacing.lg
        inWindow: true
    }
}
