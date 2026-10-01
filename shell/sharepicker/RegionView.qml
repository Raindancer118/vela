import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// Region: explained here, drawn with slurp after the picker has closed.
Item {
    id: root

    property bool shown: false

    signal activated

    opacity: shown ? 1 : 0
    visible: opacity > 0
    Behavior on opacity {
        Anim {
            duration: Theme.anim.fast
        }
    }

    Rectangle {
        id: frame

        anchors.centerIn: parent
        width: Math.min(parent.width * 0.6, 520)
        height: Math.min(parent.height * 0.8, 300)
        radius: Theme.radius.card
        color: area.containsMouse ? Theme.colors.tileHover : Theme.colors.tile

        Behavior on color {
            ColorAnim {
                duration: Theme.anim.fast
            }
        }

        // Crop marks in the corners, like a selection being drawn.
        Repeater {
            model: 4

            Item {
                required property int index
                readonly property bool atRight: index % 2 === 1
                readonly property bool atBottom: index >= 2
                readonly property int len: 28

                x: atRight ? frame.width - len - Theme.spacing.md : Theme.spacing.md
                y: atBottom ? frame.height - len - Theme.spacing.md : Theme.spacing.md
                width: len
                height: len

                Rectangle {
                    y: parent.atBottom ? parent.height - 3 : 0
                    width: parent.width
                    height: 3
                    radius: 1.5
                    color: Theme.colors.primary
                }

                Rectangle {
                    x: parent.atRight ? parent.width - 3 : 0
                    width: 3
                    height: parent.height
                    radius: 1.5
                    color: Theme.colors.primary
                }
            }
        }

        ColumnLayout {
            anchors.centerIn: parent
            width: parent.width - 2 * Theme.spacing.xl * 2
            spacing: Theme.spacing.sm

            MaterialIcon {
                Layout.alignment: Qt.AlignHCenter
                icon: "select_region"
                size: Theme.icon.huge
                color: Theme.colors.primary
            }

            StyledText {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Theme.spacing.sm
                text: I18n.tr("Draw a region")
                font.pixelSize: Theme.font.title
                font.weight: Theme.font.weightMedium
            }

            StyledText {
                Layout.fillWidth: true
                text: I18n.tr("After “Select region”, drag a rectangle with the mouse. It may not span several screens. Esc cancels.")
                color: Theme.colors.textMuted
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                elide: Text.ElideNone
            }
        }

        MouseArea {
            id: area

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.CrossCursor
            onClicked: root.activated()
        }
    }
}
