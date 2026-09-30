import QtQuick
import QtQuick.Layouts
import qs

// Centered icon + title + hint, for empty lists and missing hardware.
ColumnLayout {
    id: root

    property string icon
    property string title
    property string subtitle

    spacing: Theme.spacing.sm

    Rectangle {
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: Theme.icon.huge + 2 * Theme.spacing.lg
        implicitHeight: implicitWidth
        radius: width / 2
        color: Theme.colors.surfaceHigh

        MaterialIcon {
            anchors.centerIn: parent
            icon: root.icon
            size: Theme.icon.huge
            color: Theme.colors.textMuted
        }
    }

    StyledText {
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: Theme.spacing.xs
        text: root.title
        font.pixelSize: Theme.font.title
        font.weight: Theme.font.weightMedium
    }

    SwapText {
        Layout.alignment: Qt.AlignHCenter
        Layout.maximumWidth: parent.width
        visible: root.subtitle !== ""
        value: root.subtitle
        color: Theme.colors.textMuted
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
    }
}
