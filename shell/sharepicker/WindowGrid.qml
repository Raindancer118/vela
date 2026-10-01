import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets
import qs
import qs.components
import "logic.js" as Logic

// Shareable windows as tiles with live previews.
Item {
    id: root

    property bool shown: false
    property bool live: true
    property var windows: []
    property int totalCount: 0
    property int currentIndex: -1
    property var labelFor: w => ""
    // Few enough columns for big previews, enough for all to fit if possible.
    readonly property int infoHeight: Theme.font.body * 3 + Theme.spacing.sm
    readonly property int columns: Logic.gridColumns(windows.length, width, height, 0.62, infoHeight, 3, 5)

    signal picked(int index)
    signal activated(int index)

    opacity: shown ? 1 : 0
    visible: opacity > 0
    Behavior on opacity {
        Anim {
            duration: Theme.anim.fast
        }
    }

    onCurrentIndexChanged: if (currentIndex >= 0)
        grid.positionViewAtIndex(currentIndex, GridView.Contain)

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(parent.width, 420)
        visible: root.windows.length === 0
        icon: root.totalCount === 0 ? "select_window" : "search"
        title: root.totalCount === 0 ? I18n.tr("No windows to share") : I18n.tr("No matching windows")
        subtitle: root.totalCount === 0 ? I18n.tr("Open the window first, or share a whole screen.") : ""
    }

    GridView {
        id: grid

        anchors.fill: parent
        clip: true
        model: root.windows
        cellWidth: Math.floor(width / root.columns)
        cellHeight: Math.round(cellWidth * 0.62) + root.infoHeight
        boundsBehavior: Flickable.StopAtBounds
        currentIndex: root.currentIndex

        delegate: Item {
            id: cell

            required property var modelData
            required property int index
            readonly property bool current: root.currentIndex === index
            readonly property var entry: DesktopEntries.heuristicLookup(modelData.className)

            width: grid.cellWidth
            height: grid.cellHeight

            Rectangle {
                id: tile

                anchors.fill: parent
                anchors.margins: Theme.spacing.sm
                radius: Theme.radius.card
                color: cell.current ? Theme.colors.selected : area.containsMouse ? Theme.colors.tileHover : Theme.colors.tile
                border.width: cell.current ? 2 : Theme.size.border
                border.color: cell.current ? Theme.colors.primary : Theme.colors.outline
                scale: area.pressed ? 0.98 : 1

                Behavior on color {
                    ColorAnim {
                        duration: Theme.anim.fast
                    }
                }

                Behavior on scale {
                    SpringAnim {}
                }

                Item {
                    id: previewBox

                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: info.top
                    anchors.margins: Theme.spacing.sm

                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius.small
                        color: Theme.colors.chip
                    }

                    // Without a preview (not mapped yet, no capture): the app icon.
                    IconImage {
                        anchors.centerIn: parent
                        visible: !preview.hasContent
                        implicitSize: Theme.icon.huge
                        source: Quickshell.iconPath(cell.entry?.icon ?? cell.modelData.className, "application-x-executable")
                        opacity: 0.8
                    }

                    ScreencopyView {
                        id: preview

                        readonly property real aspect: sourceSize.width > 0 && sourceSize.height > 0 ? sourceSize.width / sourceSize.height : 16 / 10

                        anchors.centerIn: parent
                        width: Math.min(parent.width, parent.height * aspect)
                        height: Math.min(parent.height, parent.width / aspect)
                        captureSource: root.shown ? cell.modelData.toplevel : null
                        live: root.live
                        paintCursor: false
                        constraintSize: Qt.size(parent.width, parent.height)
                    }
                }

                ColumnLayout {
                    id: info

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: Theme.spacing.md
                    spacing: 2

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.sm

                        IconImage {
                            implicitSize: Theme.icon.large - 4
                            source: Quickshell.iconPath(cell.entry?.icon ?? cell.modelData.className, "application-x-executable")
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: cell.modelData.title
                            font.weight: Theme.font.weightMedium
                        }
                    }

                    StyledText {
                        Layout.fillWidth: true
                        text: [cell.entry?.name ?? cell.modelData.className, root.labelFor(cell.modelData)].filter(s => s).join(" · ")
                        font.pixelSize: Theme.font.small
                        color: Theme.colors.textMuted
                    }
                }

                MouseArea {
                    id: area

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.picked(cell.index)
                    onDoubleClicked: root.activated(cell.index)
                }
            }
        }
    }
}
