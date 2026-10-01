import QtQuick
import QtQuick.Layouts
import qs

// Quick-settings tile in the launcher's look: a tile surface, and when on
// the accent selection with its thin ring. Clicking the icon chip toggles,
// the rest opens the detail view (or toggles when there is none).
Rectangle {
    id: root

    property string icon
    property string title
    property string subtitle
    property bool active: false
    property bool hasDetail: false

    signal toggled
    signal detailRequested

    implicitHeight: Theme.size.tileHeight
    implicitWidth: Theme.size.controlWidth
    // A tall notification list must not squeeze the tiles.
    Layout.minimumHeight: Theme.size.tileHeight
    radius: Theme.radius.card
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: active ? Theme.colors.selected : body.containsMouse ? Theme.colors.tileHover : Theme.colors.tile
    border.width: Theme.size.border
    border.color: active ? Theme.colors.selectedRing : "transparent"

    Behavior on color {
        ColorAnim {}
    }

    Behavior on border.color {
        ColorAnim {}
    }

    MouseArea {
        id: body

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.hasDetail ? root.detailRequested() : root.toggled()
    }

    // Centred on the tile's middle, whatever height the grid gives it.
    RowLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Theme.spacing.md
        anchors.rightMargin: Theme.spacing.md
        spacing: Theme.spacing.md

        Rectangle {
            id: chip

            implicitWidth: Theme.size.tileIcon
            implicitHeight: Theme.size.tileIcon
            radius: Theme.radius.small
            color: root.active ? Theme.colors.accentChip : Theme.colors.chip

            Behavior on color {
                ColorAnim {}
            }

            Clickable {
                radius: chip.radius
                onClicked: root.toggled()
            }

            MaterialIcon {
                anchors.centerIn: parent
                icon: root.icon
                color: root.active ? Theme.colors.primary : Theme.colors.text

                Behavior on color {
                    ColorAnim {}
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1

            StyledText {
                Layout.fillWidth: true
                text: root.title
                font.weight: Theme.font.weightSemiBold
            }

            SwapText {
                Layout.fillWidth: true
                visible: value !== ""
                value: root.subtitle
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
            }
        }

        MaterialIcon {
            visible: root.hasDetail
            icon: "chevron_right"
            size: Theme.icon.small
            color: Theme.colors.textMuted
        }
    }
}
