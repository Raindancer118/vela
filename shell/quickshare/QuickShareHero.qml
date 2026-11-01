import QtQuick
import QtQuick.Effects
import QtQuick.Shapes
import qs
import qs.components

// The big picture in Quick Share's window. Idle: this PC in the middle,
// sending out waves, with phones and tablets circling on an orbit. During
// a transfer it turns into the two devices with a dotted line between them
// whose dots run the way the files go, and the progress below; when it is
// done, a check pops in and a flash goes out.
Item {
    id: root

    // -1: no transfer; 0..1: how much of it is through.
    property real progress: -1
    // The other device of the transfer, and which way the files go.
    property string peerName: ""
    property string peerIcon: "smartphone"
    property bool incoming: true
    readonly property bool receiving: progress >= 0
    // Sizes follow the height; the width only spreads the transfer scene.
    readonly property real span: Math.min(width, height)
    readonly property real core: span * 0.38
    readonly property bool animate: visible && Theme.anim.pulse > 0

    implicitWidth: 300
    implicitHeight: 300

    // 0..1, one cycle of the waves; faster while receiving.
    property real wave: 0
    NumberAnimation on wave {
        from: 0
        to: 1
        duration: root.receiving ? 1600 : Theme.anim.pulse
        loops: Animation.Infinite
        running: root.animate
    }

    // Shown for a moment after a transfer ended well.
    property bool done: false
    onReceivingChanged: if (!receiving && lastProgress > 0.99) {
        done = true;
        doneTimer.restart();
        flash.restart();
    }
    property real lastProgress: 0
    onProgressChanged: if (progress >= 0)
        lastProgress = progress
    Timer {
        id: doneTimer

        interval: 2600
        onTriggered: root.done = false
    }

    // Soft light behind everything: a blurred disc that breathes.
    Rectangle {
        id: halo

        anchors.centerIn: parent
        width: root.core * 1.9
        height: width
        radius: width / 2
        color: Theme.colors.primary
        opacity: root.receiving ? 0.22 : 0.1
        visible: false
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.anim.slow
            }
        }
    }
    MultiEffect {
        anchors.fill: halo
        source: halo
        blurEnabled: true
        blur: 1
        blurMax: 64
        autoPaddingEnabled: true
        opacity: halo.opacity
        scale: 1 + 0.06 * Math.sin(root.wave * 2 * Math.PI)
    }

    // Waves: three rings that grow out of the PC and fade.
    Repeater {
        model: root.receiving ? 0 : 2

        Rectangle {
            required property int index
            readonly property real phase: (root.wave + index / 2) % 1

            anchors.centerIn: parent
            width: root.core
            height: width
            radius: width / 2
            color: "transparent"
            border.width: 1.5
            border.color: Theme.colors.primary
            scale: 1 + Math.pow(phase, 0.8) * (root.span * 0.9 / root.core - 1)
            opacity: (1 - phase) * 0.4
        }
    }

    // Two orbits with devices on them; the icons stay upright.
    Repeater {
        model: [
            {
                radius: 0.42,
                speed: 48000,
                icons: ["smartphone", "tablet", "smartphone"],
                start: 30
            }
        ]

        Item {
            id: orbit

            required property var modelData
            readonly property real r: root.span * modelData.radius

            anchors.fill: parent
            visible: !root.receiving

            Rectangle {
                anchors.centerIn: parent
                width: 2 * orbit.r
                height: width
                radius: width / 2
                color: "transparent"
                border.width: 1
                border.color: Theme.withAlpha(Theme.colors.text, 0.06)
            }

            Item {
                id: spinner

                anchors.fill: parent
                rotation: orbit.modelData.start
                NumberAnimation on rotation {
                    from: orbit.modelData.start
                    to: orbit.modelData.start + 360
                    duration: root.receiving ? orbit.modelData.speed / 3 : orbit.modelData.speed
                    loops: Animation.Infinite
                    running: root.animate
                }

                Repeater {
                    model: orbit.modelData.icons

                    Rectangle {
                        required property string modelData
                        required property int index
                        readonly property real angle: 2 * Math.PI * index / orbit.modelData.icons.length

                        x: root.width / 2 + orbit.r * Math.cos(angle) - width / 2
                        y: root.height / 2 + orbit.r * Math.sin(angle) - height / 2
                        width: 26
                        height: 26
                        radius: 13
                        color: Theme.colors.chip
                        opacity: 0.8
                        rotation: -spinner.rotation

                        MaterialIcon {
                            anchors.centerIn: parent
                            icon: parent.modelData
                            size: 14
                            color: Theme.colors.textMuted
                        }
                    }
                }
            }
        }
    }

    // Flash when a transfer is done.
    Rectangle {
        id: flashRing

        anchors.centerIn: parent
        width: root.core
        height: width
        radius: width / 2
        color: Theme.withAlpha(Theme.colors.primary, 0.25)
        opacity: 0
        ParallelAnimation {
            id: flash

            NumberAnimation {
                target: flashRing
                property: "scale"
                from: 1
                to: 2.6
                duration: 900
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: flashRing
                property: "opacity"
                from: 0.9
                to: 0
                duration: 900
                easing.type: Easing.OutCubic
            }
        }
    }

    // The PC; on the right while a transfer runs.
    Item {
        id: coreItem

        width: root.core
        height: width
        x: root.receiving ? root.width * 0.86 - width / 2 : (root.width - width) / 2
        y: (root.height - height) / 2
        Behavior on x {
            NumberAnimation {
                duration: Theme.anim.slow
                easing.type: Easing.OutCubic
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: width / 2
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: Qt.lighter(Theme.colors.accentChip, 1.25)
                }
                GradientStop {
                    position: 1
                    color: Theme.colors.accentChip
                }
            }
            border.width: 1
            border.color: Theme.withAlpha(Theme.colors.primary, 0.4)
        }

        MaterialIcon {
            anchors.centerIn: parent
            icon: root.done ? "check" : "laptop"
            size: root.core * 0.42
            color: Theme.colors.primary
        }

        StyledText {
            anchors.top: parent.bottom
            anchors.topMargin: Theme.spacing.sm
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.receiving
            text: I18n.tr("This PC")
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
        }
    }

    // The other device, on the left while a transfer runs.
    Item {
        id: peer

        width: root.core
        height: width
        x: root.width * 0.14 - width / 2
        y: (root.height - height) / 2
        opacity: root.receiving ? 1 : 0
        visible: opacity > 0
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.anim.slow
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: Theme.colors.chip
            border.width: 1
            border.color: Theme.withAlpha(Theme.colors.primary, 0.4)
        }

        MaterialIcon {
            anchors.centerIn: parent
            icon: root.peerIcon
            size: root.core * 0.42
            color: Theme.colors.text
        }

        StyledText {
            anchors.top: parent.bottom
            anchors.topMargin: Theme.spacing.sm
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.core * 1.4
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: root.peerName
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
        }
    }

    // The dotted line between them; its dots run the way the files go.
    Shape {
        id: link

        readonly property real x1: peer.x + peer.width + 10
        readonly property real x2: coreItem.x - 10
        readonly property real cy: root.height / 2

        anchors.fill: parent
        opacity: root.receiving ? 1 : 0
        visible: opacity > 0
        preferredRendererType: Shape.CurveRenderer
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.anim.slow
            }
        }

        ShapePath {
            id: dots

            strokeWidth: 4
            strokeColor: Theme.colors.primary
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            strokeStyle: ShapePath.DashLine
            dashPattern: [0.01, 3]
            startX: root.incoming ? link.x1 : link.x2
            startY: link.cy
            PathLine {
                x: root.incoming ? link.x2 : link.x1
                y: link.cy
            }
            NumberAnimation on dashOffset {
                from: 3
                to: 0
                duration: 450
                loops: Animation.Infinite
                running: root.receiving && root.animate
            }
        }
    }

    // Progress under the line.
    StyledText {
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.height / 2 + 14
        visible: root.receiving
        text: Math.round(Math.max(0, root.progress) * 100) + " %"
        color: Theme.colors.primary
        font.pixelSize: Theme.font.title
        font.weight: Theme.font.weightMedium
    }
}
