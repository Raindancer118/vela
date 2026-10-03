import QtQuick
import qs
import qs.components

// A card of the overview grid. Flies in staggered by `order`, lifts a bit
// when hovered, and can be clicked.
Rectangle {
    id: root

    property int order: 0
    property bool clickable: false
    property bool shown: false
    default property alias content: inner.data
    property real padding: Theme.spacing.lg
    signal clicked

    color: area.containsMouse && clickable ? Theme.colors.tileHover : Theme.colors.tile
    radius: Theme.radius.card
    border.width: Theme.size.border
    border.color: Theme.colors.outline
    Behavior on color {
        ColorAnim {
            duration: Theme.anim.fast
        }
    }

    opacity: shown ? 1 : 0
    transform: [
        Translate {
            y: root.shown ? (area.containsMouse && root.clickable ? -2 : 0) : Theme.anim.slideShort * 1.5
            Behavior on y {
                SpringAnim {}
            }
        },
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: root.shown ? 1 : 0.97
            yScale: xScale
            Behavior on xScale {
                SpringAnim {}
            }
        }
    ]
    Behavior on opacity {
        Anim {
            duration: Theme.anim.slow
        }
    }

    Timer {
        running: true
        interval: Math.min(root.order, 10) * Theme.anim.stagger + 40
        onTriggered: root.shown = true
    }

    MouseArea {
        id: area

        anchors.fill: parent
        hoverEnabled: true
        enabled: true
        cursorShape: root.clickable ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (root.clickable) root.clicked()
    }

    Item {
        id: inner

        anchors.fill: parent
        anchors.margins: root.padding
    }
}
