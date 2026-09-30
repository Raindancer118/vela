import QtQuick
import qs

// A Material Symbols Rounded glyph. `fill` animates between the outlined
// (0) and filled (1) style; a new `icon` shrinks the old glyph away and
// springs the new one in.
Text {
    id: root

    property string icon
    property real size: Theme.icon.normal
    property real fill: 0

    property string shownIcon
    property bool ready: false

    onIconChanged: ready ? swap.restart() : shownIcon = icon
    Component.onCompleted: {
        shownIcon = icon;
        ready = true;
    }

    text: shownIcon
    color: Theme.colors.text
    font.family: Theme.font.iconFamily
    font.pixelSize: size
    font.variableAxes: ({
            "FILL": root.fill,
            "opsz": Math.max(Theme.icon.opticalSizeMin, Math.min(Theme.icon.opticalSizeMax, root.size)),
            "wght": Theme.icon.weight
        })
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter

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

    Behavior on fill {
        Anim {
            duration: Theme.anim.fast
        }
    }
}
