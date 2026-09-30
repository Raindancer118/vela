import QtQuick
import qs

// Text that slides/fades from the old to the new `value` instead of
// jumping, for status labels ("Off" -> "On").
StyledText {
    id: root

    property string value
    property bool ready: false

    onValueChanged: ready ? swap.restart() : text = value
    Component.onCompleted: {
        text = value;
        ready = true;
    }

    transform: Translate {
        id: shift
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
                target: shift
                property: "y"
                to: -Theme.anim.swapShift
                duration: Theme.anim.fast
            }
        }

        ScriptAction {
            script: {
                root.text = root.value;
                shift.y = Theme.anim.swapShift;
            }
        }

        ParallelAnimation {
            Anim {
                target: root
                property: "opacity"
                to: 1
            }

            Anim {
                target: shift
                property: "y"
                to: 0
                easing.bezierCurve: Theme.anim.emphasizedDecel
            }
        }
    }
}
