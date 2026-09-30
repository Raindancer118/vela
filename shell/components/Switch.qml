import QtQuick
import qs

// Material 3 style switch.
Rectangle {
    id: root

    property bool checked: false

    signal toggled

    implicitWidth: Theme.size.switchWidth
    implicitHeight: Theme.size.switchHeight
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: checked ? Theme.colors.primary : Theme.colors.surfaceHighest
    border.width: checked ? 0 : 2
    border.color: Theme.colors.textMuted

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        radius: root.radius
        inverted: root.checked
        onClicked: root.toggled()
    }

    Rectangle {
        property real knob: root.checked ? Theme.size.switchKnobOn : Theme.size.switchKnobOff

        width: knob
        height: knob
        radius: knob / 2
        anchors.verticalCenter: parent.verticalCenter
        // The unchecked knob is smaller; keep its center where it would be.
        x: root.checked ? root.width - knob - Theme.size.switchKnobInset : (root.height - knob) / 2
        color: root.checked ? Theme.colors.textOnPrimary : Theme.colors.textMuted

        Behavior on x {
            Anim {}
        }

        Behavior on knob {
            Anim {}
        }
    }
}
