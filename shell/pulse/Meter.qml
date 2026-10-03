import QtQuick
import qs
import qs.components

// Thin horizontal bar; several `segments` ({ value, color }) stack.
Item {
    id: root

    property real value: 0
    property real max: 100
    property color color: Theme.colors.primary
    property var segments: []
    readonly property real radius: height / 2

    implicitHeight: 6

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: Theme.withAlpha(Theme.colors.text, 0.08)
    }

    Row {
        clip: true
        anchors.fill: parent

        Repeater {
            // A count, not the array: the bars stay and glide to new values
            // instead of being made anew (from 0) with every update.
            model: Math.max(1, root.segments.length)

            Rectangle {
                required property int index
                readonly property var modelData: root.segments.length > 0 ? (root.segments[index] ?? { value: 0, color: root.color }) : { value: root.value, color: root.color }

                height: root.height
                width: Math.max(0, Math.min(1, modelData.value / root.max)) * root.width
                color: modelData.color
                radius: root.segments.length > 0 ? 0 : root.radius
                Behavior on width {
                    SpringAnim {
                        duration: Theme.anim.slow
                    }
                }
                Behavior on color {
                    ColorAnim {}
                }
            }
        }
    }
}
