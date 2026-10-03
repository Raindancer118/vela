import QtQuick
import qs
import qs.components

// A number that rolls to its new value instead of jumping.
StyledText {
    id: root

    property real value: 0
    property var format: v => Math.round(v).toString()
    property real shown: value
    Behavior on shown {
        NumberAnimation {
            duration: Theme.anim.slow
            easing.type: Easing.OutCubic
        }
    }

    text: format(shown)
    font.features: { "tnum": 1 }
}
