import QtQuick
import qs
import qs.components

// Scroll bar for a Flickable or ListView: shows where you are, widens under
// the pointer, and can be dragged or clicked to jump.
Item {
    id: root

    required property Flickable flick
    readonly property bool needed: flick.contentHeight > flick.height + 1
    readonly property bool active: area.containsMouse || area.pressed || flick.moving

    anchors.top: flick.top
    anchors.bottom: flick.bottom
    anchors.right: flick.right
    width: 14
    visible: needed
    z: 50

    Rectangle {
        id: handle

        readonly property real ratio: Math.min(1, root.flick.visibleArea.heightRatio)
        anchors.right: parent.right
        anchors.rightMargin: 2
        width: area.containsMouse || area.pressed ? 8 : 4
        height: Math.max(28, ratio * root.height)
        y: Math.max(0, Math.min(root.height - height, root.flick.visibleArea.yPosition * root.height))
        radius: width / 2
        color: area.pressed ? Theme.colors.textMuted : Theme.colors.textDisabled
        opacity: root.active ? 1 : Theme.opacity.scrollbarIdle
        Behavior on width {
            Anim {
                duration: Theme.anim.fast
            }
        }
        Behavior on opacity {
            Anim {
                duration: Theme.anim.fast
            }
        }
    }

    MouseArea {
        id: area

        property real grab: 0

        anchors.fill: parent
        hoverEnabled: true
        preventStealing: true
        function scrollTo(y: real): void {
            const f = root.flick;
            const max = Math.max(0, f.contentHeight - f.height);
            const top = Math.max(0, Math.min(1, (y - grab) / Math.max(1, root.height - handle.height)));
            f.contentY = f.originY + top * max;
        }
        onPressed: mouse => {
            // On the handle: drag it; elsewhere: jump there.
            grab = mouse.y >= handle.y && mouse.y <= handle.y + handle.height ? mouse.y - handle.y : handle.height / 2;
            scrollTo(mouse.y);
        }
        onPositionChanged: mouse => {
            if (pressed)
                scrollTo(mouse.y);
        }
        onWheel: wheel => {
            root.flick.flick(0, wheel.angleDelta.y * 8);
        }
    }
}
