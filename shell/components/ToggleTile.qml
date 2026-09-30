import QtQuick
import QtQuick.Layouts
import qs

// Quick-settings tile: clicking the round icon toggles, clicking the rest
// opens the detail view (or toggles when there is none).
Rectangle {
    id: root

    property string icon
    property string title
    property string subtitle
    property bool active: false
    property bool hasDetail: false

    readonly property color contentColor: active ? Theme.colors.textOnPrimary : Theme.colors.text

    signal toggled
    signal detailRequested

    implicitHeight: Theme.size.tileHeight
    implicitWidth: Theme.size.controlWidth
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: active ? Theme.colors.primary : Theme.colors.surfaceHigh

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        radius: root.radius
        inverted: root.active
        onClicked: root.hasDetail ? root.detailRequested() : root.toggled()
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: (Theme.size.tileHeight - Theme.size.tileIcon) / 2
        anchors.rightMargin: Theme.spacing.md
        spacing: Theme.spacing.sm

        Rectangle {
            id: iconCircle

            implicitWidth: Theme.size.tileIcon
            implicitHeight: Theme.size.tileIcon
            radius: width / 2
            color: root.active ? Theme.colors.tileIconActive : Theme.colors.tileIcon

            Behavior on color {
                ColorAnim {}
            }

            Clickable {
                radius: iconCircle.radius
                inverted: root.active
                onClicked: root.toggled()
            }

            MaterialIcon {
                anchors.centerIn: parent
                icon: root.icon
                fill: root.active ? 1 : 0
                color: root.contentColor
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            StyledText {
                Layout.fillWidth: true
                text: root.title
                color: root.contentColor
                font.weight: Theme.font.weightMedium
            }

            SwapText {
                Layout.fillWidth: true
                visible: value !== ""
                value: root.subtitle
                color: root.active ? Theme.colors.textOnPrimaryMuted : Theme.colors.textMuted
                font.pixelSize: Theme.font.small
            }
        }

        MaterialIcon {
            visible: root.hasDetail
            icon: "chevron_right"
            size: Theme.icon.small
            color: root.active ? Theme.colors.textOnPrimaryMuted : Theme.colors.textMuted
        }
    }
}
