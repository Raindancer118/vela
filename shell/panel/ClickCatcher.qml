import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.services

// While the panel is open, an invisible surface on every other monitor
// closes it on click (on the panel's own monitor ControlCenter does that).
// Part of the focus grab in shell.qml, so the closing click is swallowed
// like on the panel's monitor instead of reaching the window below.
Variants {
    model: Quickshell.screens

    PanelWindow {
        required property ShellScreen modelData

        screen: modelData
        visible: ShellState.panelOpen && modelData !== ShellState.panelScreen
        color: "transparent"
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "quickshell-catcher"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        MouseArea {
            anchors.fill: parent
            // On press: the press moves keyboard focus away from the panel,
            // which cancels the click before it would complete.
            onPressed: ShellState.closePanel()
        }
    }
}
