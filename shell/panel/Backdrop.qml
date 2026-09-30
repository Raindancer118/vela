import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.services

// Optional blurred, dimmed layers behind the panel on its monitor (vela
// settings → Appearance → Blur). Hyprland blurs and fades them via the
// "vela-shell-backdrop" layer rule from vela.lua; they never take input.
// `strength` of them are stacked: each blurs the result below again.
Variants {
    model: Array.from({ length: Config.panelBackdropLayers }, (_, i) => i)

    PanelWindow {
        required property int modelData

        visible: Config.panelBackdrop && ShellState.panelOpen && ShellState.panelScreen !== null
        screen: ShellState.panelScreen
        color: Qt.rgba(0, 0, 0, Config.panelBackdropLayerAlpha)
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        exclusionMode: ExclusionMode.Ignore
        mask: Region {}
        // Top: always below the panel (Overlay), above windows.
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "vela-shell-backdrop"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }
}
