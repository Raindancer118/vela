import QtQuick
import qs

// Fills its parent, handles clicks and draws a hover/press state layer.
// Declare it before the parent's content so the layer sits beneath it.
MouseArea {
    id: root

    property real radius: 0
    property bool inverted: false

    anchors.fill: parent
    hoverEnabled: true
    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: {
            if (!root.enabled)
                return "transparent";
            if (root.pressed)
                return root.inverted ? Theme.colors.pressedOnPrimary : Theme.colors.pressed;
            if (root.containsMouse)
                return root.inverted ? Theme.colors.hoverOnPrimary : Theme.colors.hover;
            return "transparent";
        }

        Behavior on color {
            ColorAnim {
                duration: Theme.anim.fast
            }
        }
    }
}
