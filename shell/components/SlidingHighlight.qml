import QtQuick
import qs

// Background of the selected row that glides to a new `target` (a sibling
// row laid out in the same coordinates) instead of each row switching its
// own background. First placement is instant.
Rectangle {
    id: root

    property Item target: null
    property bool placed: false

    visible: target !== null
    x: target?.x ?? 0
    y: target?.y ?? 0
    width: target?.width ?? 0
    height: target?.height ?? 0
    radius: Theme.radius.small
    color: Theme.colors.surfaceHighest

    onTargetChanged: {
        if (target && !placed)
            Qt.callLater(() => root.placed = true);
    }

    Behavior on y {
        enabled: root.placed

        SpringAnim {
            duration: Theme.anim.normal
        }
    }

    Behavior on height {
        enabled: root.placed

        Anim {}
    }
}
