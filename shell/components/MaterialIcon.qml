import QtQuick
import QtQuick.Effects
import qs

// A symbolic icon from assets/icons (Adwaita, like GTK and the vela
// settings), drawn in `color`. A new `icon` shrinks the old one away and
// springs the new one in. (The name stays from the Material Symbols days.)
Item {
    id: root

    property string icon
    property real size: Theme.icon.normal
    property color color: Theme.colors.text
    // Kept for callers; symbolic icons have no filled style.
    property real fill: 0

    property string shownIcon
    property bool ready: false

    onIconChanged: ready ? swap.restart() : shownIcon = icon
    Component.onCompleted: {
        shownIcon = icon;
        ready = true;
    }

    implicitWidth: size
    implicitHeight: size

    Image {
        id: glyph

        anchors.centerIn: parent
        width: root.size
        height: root.size
        source: root.shownIcon !== "" ? Qt.resolvedUrl("../assets/icons/" + root.shownIcon + ".svg") : ""
        sourceSize: Qt.size(root.size * 2, root.size * 2)
        smooth: true
        visible: false
    }

    // Adwaita's symbolic icons are black: lifted to white, then tinted, so
    // the result is exactly `color` with the icon's own anti-aliasing.
    MultiEffect {
        anchors.fill: glyph
        source: glyph
        visible: glyph.status === Image.Ready
        brightness: 1
        colorization: 1
        colorizationColor: root.color
    }

    SequentialAnimation {
        id: swap

        ParallelAnimation {
            Anim {
                target: root
                property: "opacity"
                to: 0
                duration: Theme.anim.fast
            }

            Anim {
                target: root
                property: "scale"
                to: Theme.anim.swapScale
                duration: Theme.anim.fast
            }
        }

        ScriptAction {
            script: root.shownIcon = root.icon
        }

        ParallelAnimation {
            Anim {
                target: root
                property: "opacity"
                to: 1
                duration: Theme.anim.fast
            }

            SpringAnim {
                target: root
                property: "scale"
                to: 1
            }
        }
    }
}
