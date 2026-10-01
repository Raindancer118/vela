import QtQuick
import qs

// Switch in the libadwaita style of vela's settings window: pill track
// (grey when off, accent when on) and a white knob of the same size in
// both states.
Rectangle {
    id: root

    property bool checked: false

    signal toggled

    implicitWidth: Theme.size.switchWidth
    implicitHeight: Theme.size.switchHeight
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: checked ? Theme.colors.primary : Theme.withAlpha(Theme.colors.text, 0.15)

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        radius: root.radius
        inverted: root.checked
        onClicked: root.toggled()
    }

    // Soft shadow under the knob.
    Rectangle {
        x: knob.x
        y: knob.y + 1
        width: knob.width
        height: knob.height
        radius: knob.radius
        scale: knob.scale
        color: Qt.rgba(0, 0, 0, 0.22)
    }

    Rectangle {
        id: knob

        width: Theme.size.switchKnob
        height: width
        radius: width / 2
        anchors.verticalCenter: parent.verticalCenter
        x: root.checked ? root.width - width - Theme.size.switchKnobInset : Theme.size.switchKnobInset
        color: "#ffffff"

        Behavior on x {
            SpringAnim {
                duration: Theme.anim.normal
            }
        }

        // Switching on: the knob gives a small springy pop.
        SequentialAnimation {
            id: pop

            NumberAnimation {
                target: knob
                property: "scale"
                to: 1.18
                duration: Theme.anim.fast
                easing.type: Easing.OutCubic
            }

            SpringAnim {
                target: knob
                property: "scale"
                to: 1
            }
        }
    }

    onCheckedChanged: {
        if (checked)
            pop.restart();
    }
}
