import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// Small label pill (status, kind, flags).
Rectangle {
    id: root

    property string text
    property string icon
    property color tint: Theme.colors.text
    property bool strong: false

    implicitHeight: 20
    implicitWidth: row.implicitWidth + 2 * Theme.spacing.sm
    radius: height / 2
    color: Theme.withAlpha(tint, strong ? 0.22 : 0.10)
    Behavior on color {
        ColorAnim {}
    }

    RowLayout {
        id: row

        anchors.centerIn: parent
        spacing: 3

        MaterialIcon {
            visible: root.icon !== ""
            icon: root.icon
            size: 12
            color: root.tint
        }

        StyledText {
            text: root.text
            color: root.tint
            font.pixelSize: Theme.font.small - 1
            font.weight: Theme.font.weightMedium
        }
    }
}
