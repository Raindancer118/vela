import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.components

// One OSD per monitor: a pill of dots at the top center that appears when
// that monitor's active workspace changes and fades out afterwards.
// It is click-through, reserves no space and never takes focus.
Variants {
    model: Quickshell.screens

    PanelWindow {
        id: root

        required property ShellScreen modelData

        // Looked up by name (reactive, unlike Hyprland.monitorFor()).
        // modelData becomes null briefly while its screen is being removed.
        readonly property HyprlandMonitor monitor: Hyprland.monitors.values.find(m => m.name === modelData?.name) ?? null
        readonly property int activeId: monitor?.activeWorkspace?.id ?? 0
        property bool shown: false

        // Workspace ids that have windows.
        readonly property var occupied: Hyprland.workspaces.values.filter(w => w.id > 0 && w.toplevels.values.length > 0).map(w => w.id)
        readonly property int dotCount: {
            const highest = Math.max(...Hyprland.workspaces.values.map(w => w.id));
            const count = Math.min(Config.osdMaxWorkspaces, Math.max(Config.osdMinWorkspaces, highest));
            // Always include the active workspace, even above the maximum.
            return Math.max(count, activeId);
        }

        function show(): void {
            shown = true;
            hideTimer.restart();
        }

        // Only real switches count: the value at creation (login, reload,
        // monitor plugged in) and special/named workspaces (id <= 0) don't.
        property int lastId: 0
        property int lastFocusedId: 0

        Component.onCompleted: {
            lastId = activeId;
            lastFocusedId = Hyprland.focusedWorkspace?.id ?? 0;
        }

        onActiveIdChanged: {
            if (lastId > 0 && activeId > 0)
                show();
            lastId = activeId;
        }

        // `workspace N` where N is already visible on another monitor only
        // moves focus; no monitor's active workspace changes.
        Connections {
            target: Hyprland

            function onFocusedWorkspaceChanged(): void {
                const workspace = Hyprland.focusedWorkspace;
                const id = workspace?.id ?? 0;
                if (root.lastFocusedId > 0 && id > 0 && workspace.monitor?.name === root.modelData?.name)
                    root.show();
                root.lastFocusedId = id;
            }
        }

        screen: modelData
        visible: shown || pill.opacity > 0
        color: "transparent"
        anchors.top: true
        margins.top: Theme.size.osdTopMargin
        // Sized for the maximum dot count so the surface never resizes.
        implicitWidth: Config.osdMaxWorkspaces * (Theme.size.osdDot + Theme.size.osdDotSpacing) + Theme.size.osdDotActive + 2 * Theme.size.osdPadding
        implicitHeight: pill.height
        exclusionMode: ExclusionMode.Ignore
        mask: Region {}
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "quickshell-osd"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        Timer {
            id: hideTimer

            interval: Config.osdTimeout
            onTriggered: root.shown = false
        }

        Rectangle {
            id: pill

            anchors.horizontalCenter: parent.horizontalCenter
            width: dots.implicitWidth + 2 * Theme.size.osdPadding
            height: Theme.size.osdDot + 2 * Theme.size.osdPadding
            radius: height / 2
            color: Theme.colors.panel
            border.width: Theme.size.border
            border.color: Theme.colors.outline
            opacity: root.shown ? 1 : 0
            scale: root.shown ? 1 : Theme.anim.popInScale

            Behavior on opacity {
                Anim {}
            }

            Behavior on scale {
                Anim {}
            }

            Behavior on width {
                Anim {
                    duration: Theme.anim.fast
                }
            }

            Row {
                id: dots

                anchors.centerIn: parent
                spacing: Theme.size.osdDotSpacing

                Repeater {
                    model: root.dotCount

                    Rectangle {
                        required property int index

                        readonly property int workspace: index + 1
                        readonly property bool active: workspace === root.activeId

                        width: active ? Theme.size.osdDotActive : Theme.size.osdDot
                        height: Theme.size.osdDot
                        radius: height / 2
                        color: active ? Theme.colors.primary : root.occupied.includes(workspace) ? Theme.colors.dotOccupied : Theme.colors.dotEmpty

                        Behavior on width {
                            Anim {}
                        }

                        Behavior on color {
                            ColorAnim {}
                        }
                    }
                }
            }
        }
    }
}
