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
            model: root.segments.length > 0 ? root.segments : [{ value: root.value, color: root.color }]

            Rectangle {
                required property var modelData
                required property int index

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
