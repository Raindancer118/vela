import QtQuick
import qs
import qs.components

// Dimmed backdrop with a centred card for dialogs; the card springs in.
Item {
    id: root

    property bool open: false
    default property alias content: card.data
    property real cardWidth: 460
    property real cardHeight: card.childrenRect.height + 2 * Theme.spacing.xl
    signal dismissed

    visible: opacity > 0
    opacity: open ? 1 : 0
    Behavior on opacity {
        Anim {
            duration: Theme.anim.fast
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.35)

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismissed()
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: root.cardWidth
        height: root.cardHeight
        radius: Theme.radius.panel
        color: Theme.colors.background
        border.width: 1
        border.color: Theme.colors.outline
        scale: root.open ? 1 : 0.94
        Behavior on scale {
            SpringAnim {}
        }

        MouseArea {
            anchors.fill: parent
        }

        Item {
            id: card

            anchors.fill: parent
            anchors.margins: Theme.spacing.xl
        }
    }
}
