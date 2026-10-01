import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.components
import "logic.js" as Logic

// The monitors as they stand on the desk, each with a live preview.
Item {
    id: root

    property bool shown: false
    property bool live: true
    property string selected

    signal picked(string name)
    signal activated(string name)

    readonly property int labelHeight: Theme.font.body + Theme.font.small + Theme.spacing.md * 2
    readonly property var rects: Logic.screenLayout(Quickshell.screens.map(s => ({
                    name: s.name,
                    x: s.x,
                    y: s.y,
                    width: s.width,
                    height: s.height
                })), width, height - labelHeight, Theme.spacing.lg)

    function move(direction: string): void {
        const next = Logic.nearestScreen(rects, rects.findIndex(r => r.name === selected), direction);
        if (next >= 0)
            picked(rects[next].name);
    }

    opacity: shown ? 1 : 0
    visible: opacity > 0
    Behavior on opacity {
        Anim {
            duration: Theme.anim.fast
        }
    }

    Repeater {
        model: Quickshell.screens

        Item {
            id: tile

            required property ShellScreen modelData
            readonly property var rect: root.rects.find(r => r.name === modelData.name) ?? null
            readonly property bool current: root.selected === modelData.name
            readonly property bool focusedMonitor: Hyprland.monitors.values.find(m => m.name === modelData.name)?.focused ?? false

            visible: rect !== null
            x: rect?.x ?? 0
            y: rect?.y ?? 0
            width: rect?.width ?? 0
            height: (rect?.height ?? 0) + root.labelHeight

            Rectangle {
                id: frame

                width: parent.width
                height: tile.rect?.height ?? 0
                radius: Theme.radius.card
                color: tile.current ? Theme.colors.selected : area.containsMouse ? Theme.colors.tileHover : Theme.colors.tile
                border.width: tile.current ? 2 : Theme.size.border
                border.color: tile.current ? Theme.colors.primary : Theme.colors.outline
                scale: area.pressed ? 0.98 : 1

                Behavior on color {
                    ColorAnim {
                        duration: Theme.anim.fast
                    }
                }

                Behavior on scale {
                    SpringAnim {}
                }

                ScreencopyView {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.sm
                    captureSource: root.shown ? tile.modelData : null
                    live: root.live
                    paintCursor: false
                    constraintSize: Qt.size(width, height)
                }

                MouseArea {
                    id: area

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.picked(tile.modelData.name)
                    onDoubleClicked: root.activated(tile.modelData.name)
                }

                // Selected: a check badge in the corner.
                Rectangle {
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.margins: Theme.spacing.md
                    implicitWidth: Theme.size.iconButton - Theme.spacing.sm
                    implicitHeight: implicitWidth
                    radius: width / 2
                    color: Theme.colors.primary
                    scale: tile.current ? 1 : 0
                    opacity: tile.current ? 1 : 0

                    Behavior on scale {
                        SpringAnim {}
                    }

                    Behavior on opacity {
                        Anim {
                            duration: Theme.anim.fast
                        }
                    }

                    MaterialIcon {
                        anchors.centerIn: parent
                        icon: "check"
                        size: Theme.icon.normal
                        color: Theme.colors.textOnPrimary
                    }
                }
            }

            Column {
                anchors.top: frame.bottom
                anchors.topMargin: Theme.spacing.sm
                width: parent.width
                spacing: 0

                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.spacing.sm

                    StyledText {
                        width: Math.min(implicitWidth, tile.width - (activeChip.visible ? activeChip.width + Theme.spacing.sm : 0))
                        anchors.verticalCenter: parent.verticalCenter
                        text: Logic.screenTitle(tile.modelData.name, tile.modelData.model, I18n.tr("Built-in display"))
                        font.weight: Theme.font.weightMedium
                        color: tile.current ? Theme.colors.text : Theme.colors.textMuted
                    }

                    Rectangle {
                        id: activeChip

                        anchors.verticalCenter: parent.verticalCenter
                        visible: tile.focusedMonitor
                        implicitWidth: activeText.implicitWidth + Theme.spacing.md
                        implicitHeight: activeText.implicitHeight + 2
                        radius: height / 2
                        color: Theme.colors.accentChip

                        StyledText {
                            id: activeText

                            anchors.centerIn: parent
                            text: I18n.tr("Active")
                            font.pixelSize: Theme.font.small
                            font.weight: Theme.font.weightSemiBold
                            color: Theme.colors.primary
                        }
                    }
                }

                StyledText {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: tile.modelData.name + " · " + tile.modelData.width + "×" + tile.modelData.height
                    font.pixelSize: Theme.font.small
                    color: Theme.colors.textDisabled
                }
            }
        }
    }
}
