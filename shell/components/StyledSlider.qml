import QtQuick
import qs

// Wide pill slider: the filled part is light and always at least as wide as
// the bar is high, so the icon at its start stays readable on it.
Item {
    id: root

    property real value: 0
    property string icon
    property bool dimmed: false
    property bool showValue: true
    property bool iconInteractive: true
    property real wheelStep: Config.sliderWheelStep
    readonly property bool dragging: dragArea.pressed
    readonly property real shownValue: dragging ? dragValue : Math.max(0, Math.min(1, value))
    property real dragValue: 0

    signal moved(real value)
    signal iconClicked

    function valueAt(x: real): real {
        const span = width - height;
        return span <= 0 ? 0 : Math.max(0, Math.min(1, (x - height / 2) / span));
    }

    implicitHeight: Theme.size.sliderHeight
    implicitWidth: Theme.size.controlWidth

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: Theme.colors.surfaceHigh
    }

    Rectangle {
        id: fill

        height: parent.height
        width: root.height + root.shownValue * (root.width - root.height)
        radius: height / 2
        color: root.dimmed ? Theme.colors.primaryMuted : Theme.colors.primary

        Behavior on width {
            enabled: !root.dragging

            Anim {
                duration: Theme.anim.fast
            }
        }

        Behavior on color {
            ColorAnim {}
        }
    }

    MouseArea {
        id: dragArea

        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        preventStealing: true
        onPressed: mouse => {
            root.dragValue = root.valueAt(mouse.x);
            root.moved(root.dragValue);
        }
        onPositionChanged: mouse => {
            if (!pressed)
                return;
            root.dragValue = root.valueAt(mouse.x);
            root.moved(root.dragValue);
        }
        onWheel: wheel => {
            const steps = wheel.angleDelta.y / 120;
            // Values above 100% (set elsewhere) are not pulled down by scrolling up.
            if (steps > 0 && root.value >= 1)
                return;
            root.moved(Math.max(0, Math.min(1, root.value + steps * root.wheelStep)));
        }
    }

    Rectangle {
        id: iconBox

        width: root.height
        height: root.height
        radius: height / 2
        color: "transparent"

        Clickable {
            enabled: root.iconInteractive
            radius: iconBox.radius
            inverted: true
            onClicked: root.iconClicked()
        }

        MaterialIcon {
            anchors.centerIn: parent
            icon: root.icon
            size: root.height > Theme.size.sliderHeightCompact ? Theme.icon.normal : Theme.icon.small
            fill: root.dimmed ? 0 : 1
            color: Theme.colors.textOnPrimary
        }
    }

    StyledText {
        visible: root.showValue
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.lg
        anchors.verticalCenter: parent.verticalCenter
        text: Math.round(root.shownValue * 100) + "%"
        // Readable on both halves: sits on the track until the fill reaches it.
        color: fill.width > root.width - width - Theme.spacing.lg ? Theme.colors.textOnPrimary : Theme.colors.textMuted
        font.pixelSize: Theme.font.small
        font.weight: Theme.font.weightMedium
    }
}
