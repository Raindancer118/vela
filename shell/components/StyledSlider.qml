import QtQuick
import QtQuick.Layouts
import qs

// Slider like GTK's in the vela settings: an icon button, a thin track with
// the accent fill and a light round knob, the value on the right.
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
        const span = track.width - Theme.size.sliderKnob;
        return span <= 0 ? 0 : Math.max(0, Math.min(1, (x - Theme.size.sliderKnob / 2) / span));
    }

    implicitHeight: Theme.size.sliderHeight
    implicitWidth: Theme.size.controlWidth

    RowLayout {
        anchors.fill: parent
        spacing: Theme.spacing.sm

        Rectangle {
            id: iconBox

            Layout.preferredWidth: Theme.size.iconButton
            Layout.preferredHeight: Theme.size.iconButton
            radius: height / 2
            color: "transparent"

            Clickable {
                enabled: root.iconInteractive
                radius: iconBox.radius
                onClicked: root.iconClicked()
            }

            MaterialIcon {
                anchors.centerIn: parent
                icon: root.icon
                color: root.dimmed ? Theme.colors.textMuted : Theme.colors.text
            }
        }

        Item {
            id: track

            Layout.fillWidth: true
            Layout.fillHeight: true

            Rectangle {
                id: groove

                anchors.verticalCenter: parent.verticalCenter
                x: Theme.size.sliderKnob / 2
                width: parent.width - Theme.size.sliderKnob
                height: Theme.size.sliderTrack
                radius: height / 2
                color: Theme.colors.surfaceHighest
            }

            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                x: groove.x
                width: knob.x + knob.width / 2 - groove.x
                height: Theme.size.sliderTrack
                radius: height / 2
                color: root.dimmed ? Theme.colors.primaryMuted : Theme.colors.primary

                Behavior on color {
                    ColorAnim {}
                }
            }

            // Soft shadow under the knob.
            Rectangle {
                x: knob.x
                y: knob.y + 1
                width: knob.width
                height: knob.height
                radius: width / 2
                color: Qt.rgba(0, 0, 0, Theme.va.light ? 0.18 : 0.35)
            }

            Rectangle {
                id: knob

                anchors.verticalCenter: parent.verticalCenter
                x: root.shownValue * (track.width - width)
                width: Theme.size.sliderKnob
                height: Theme.size.sliderKnob
                radius: width / 2
                color: Theme.colors.knob
                border.width: Theme.size.border
                border.color: Qt.rgba(0, 0, 0, 0.12)
                scale: root.dragging ? 1.1 : dragArea.containsMouse ? 1.05 : 1

                Behavior on x {
                    enabled: !root.dragging

                    Anim {
                        duration: Theme.anim.fast
                    }
                }

                Behavior on scale {
                    Anim {
                        duration: Theme.anim.fast
                    }
                }
            }

            MouseArea {
                id: dragArea

                anchors.fill: parent
                hoverEnabled: true
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
        }

        StyledText {
            visible: root.showValue
            Layout.preferredWidth: Theme.size.usagePercentWidth
            horizontalAlignment: Text.AlignRight
            text: Math.round(root.shownValue * 100) + " %"
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
            font.features: ({
                    "tnum": 1
                })
        }
    }
}
