import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.services

// Optional blurred, dimmed layer behind the panel on its monitor (vela
// settings → Appearance → Blur). Hyprland blurs and fades it via the
// "vela-shell-backdrop" layer rule from vela.lua; it never takes input.
PanelWindow {
    visible: Config.panelBackdrop && ShellState.panelOpen && ShellState.panelScreen !== null
    screen: ShellState.panelScreen
    color: Theme.colors.backdrop
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
