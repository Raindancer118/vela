import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import "Words.js" as W

// A stick or card in the sidebar: what it is, how big, what is on it.
Item {
    id: root

    required property var drive
    property bool selected: false
    signal picked

    implicitHeight: 64

    Clickable {
        radius: Theme.radius.small
        onClicked: root.picked()
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.md
        anchors.rightMargin: Theme.spacing.md
        spacing: Theme.spacing.md

        Rectangle {
            implicitWidth: 36
            implicitHeight: 36
            radius: 11
            color: root.selected ? Theme.colors.accentChip : Theme.colors.chip
            Behavior on color {
                ColorAnim {}
            }

            MaterialIcon {
                anchors.centerIn: parent
                icon: DrivesText.icon(root.drive)
                size: 20
                color: root.selected ? Theme.colors.primary : Theme.colors.text
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1

            StyledText {
                Layout.fillWidth: true
                text: DrivesText.name(root.drive)
                font.weight: root.selected ? Theme.font.weightMedium : Theme.font.weightNormal
                elide: Text.ElideRight
            }

            StyledText {
                Layout.fillWidth: true
                text: W.nominal(root.drive.size, Qt.locale()) + "  ·  " + DrivesText.contents(root.drive)
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
                elide: Text.ElideRight
            }
        }
    }
}
