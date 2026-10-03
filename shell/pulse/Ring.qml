import QtQuick
import QtQuick.Shapes
import qs
import qs.components

// Circular gauge; the arc springs to new values and the colour follows.
Item {
    id: root

    property real value: 0
    property real max: 100
    property color color: Theme.colors.primary
    property real thickness: 8
    property color track: Theme.withAlpha(Theme.colors.text, 0.08)
    // Soft glow that breathes while `alert` (critical states).
    property bool alert: false
    default property alias content: center.data

    property real shown: 0
    Behavior on shown {
        SpringAnim {
            duration: Theme.anim.slow * 2
        }
    }
    onValueChanged: shown = Math.max(0, Math.min(1, value / max))
    Component.onCompleted: shown = Math.max(0, Math.min(1, value / max))

    implicitWidth: 96
    implicitHeight: 96

    Rectangle {
        anchors.centerIn: parent
        width: parent.width * 0.9
        height: width
        radius: width / 2
        color: Theme.withAlpha(root.color, 0.18)
        visible: root.alert
        opacity: 0
        SequentialAnimation on opacity {
            running: root.alert && Theme.anim.normal > 0
            loops: Animation.Infinite
            NumberAnimation { to: 1; duration: 1300; easing.type: Easing.InOutSine }
            NumberAnimation { to: 0; duration: 1300; easing.type: Easing.InOutSine }
        }
        scale: 1.08
    }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeWidth: root.thickness
            strokeColor: root.track
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.width / 2 - root.thickness / 2
                radiusY: root.height / 2 - root.thickness / 2
                startAngle: 135
                sweepAngle: 270
            }
        }

        ShapePath {
            strokeWidth: root.thickness
            strokeColor: root.color
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            Behavior on strokeColor {
                ColorAnim {}
            }
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.width / 2 - root.thickness / 2
                radiusY: root.height / 2 - root.thickness / 2
                startAngle: 135
                sweepAngle: Math.max(0.5, 270 * root.shown)
            }
        }
    }

    Item {
        id: center

        anchors.fill: parent
    }
}
