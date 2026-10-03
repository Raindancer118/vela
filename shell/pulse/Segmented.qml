import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// A row of choices with a selection that glides between them.
Rectangle {
    id: root

    // [{ k, t, i? }]
    property var options: []
    property string current: ""
    signal picked(string k)

    implicitHeight: 30
    implicitWidth: row.implicitWidth + 6
    radius: height / 2
    color: Theme.withAlpha(Theme.colors.text, 0.05)

    Rectangle {
        readonly property Item target: rep.itemAt(root.options.findIndex(o => o.k === root.current))
        visible: target !== null
        x: 3 + (target?.x ?? 0)
        y: 3
        width: target?.width ?? 0
        height: parent.height - 6
        radius: height / 2
        color: Theme.colors.selected
        border.width: 1
        border.color: Theme.colors.selectedRing
        Behavior on x {
            SpringAnim {}
        }
        Behavior on width {
            SpringAnim {}
        }
    }

    Row {
        id: row

        x: 3
        y: 3
        height: parent.height - 6

        Repeater {
            id: rep

            model: root.options.length

            Item {
                required property int index
                readonly property var opt: root.options[index] ?? ({ k: "", t: "" })

                width: inner.implicitWidth + 2 * Theme.spacing.md
                height: parent.height

                Clickable {
                    radius: height / 2
                    onClicked: root.picked(parent.opt.k)
                }

                RowLayout {
                    id: inner

                    anchors.centerIn: parent
                    spacing: Theme.spacing.xs

                    MaterialIcon {
                        visible: (parent.parent.opt.i ?? "") !== ""
                        icon: parent.parent.opt.i ?? ""
                        size: Theme.icon.small
                        color: root.current === parent.parent.opt.k ? Theme.colors.primary : Theme.colors.textMuted
                    }

                    StyledText {
                        text: parent.parent.opt.t
                        font.pixelSize: Theme.font.small + 1
                    }
                }
            }
        }
    }
}
