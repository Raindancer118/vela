import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.services

// Optional blurred, dimmed layers behind the panel on its monitor (vela
// settings → Appearance → Blur). Hyprland blurs and fades them via the
// "vela-shell-backdrop" layer rule from vela.lua; they never take input.
// `strength` of them are stacked: each blurs the result below again.
// Without blur: one layer in "vela-shell-backdrop-dim" (no blur rule). The
// namespace is in the model so switching recreates the surfaces.
Variants {
    model: Array.from({ length: Config.panelBackdropLayers }, (_, i) => (Config.panelBackdropBlur ? "vela-shell-backdrop" : "vela-shell-backdrop-dim") + "#" + i)

    PanelWindow {
        required property string modelData

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
        WlrLayershell.namespace: modelData.split("#")[0]
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }
}
