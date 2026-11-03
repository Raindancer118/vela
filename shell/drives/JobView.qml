import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse

// What a job is doing: a ring that fills while bytes move, then a check,
// a cross or a stop sign. Covers the window while it runs.
Scrim {
    id: root

    readonly property var job: Drives.job
    property var shown: null
    onJobChanged: if (job) shown = job

    readonly property string phase: shown?.phase ?? ""
    readonly property bool running: Drives.busy
    readonly property bool ok: phase === "done"
    readonly property bool failed: phase === "failed"
    readonly property bool hasBytes: (shown?.total ?? 0) > 0 && ["write", "verify", "copy", "erase"].includes(phase)
    readonly property color tone: failed ? Theme.pulse.crit : ok ? Theme.pulse.ok : phase === "verify" ? Theme.pulse.net : Theme.colors.primary

    open: job !== null
    cardWidth: 440
    // Running jobs can't be clicked away; finished ones can.
    onDismissed: Drives.dismissJob()

    ColumnLayout {
        width: parent.width
        spacing: Theme.spacing.md

        Ring {
            id: ring

            Layout.alignment: Qt.AlignHCenter
            implicitWidth: 132
            implicitHeight: 132
            thickness: 9
            max: 1
            value: root.ok ? 1 : root.hasBytes ? root.shown.done / root.shown.total : 0
            color: root.tone
            alert: root.failed

            // No byte count yet (unmounting, the password): the ring breathes.
            Rectangle {
                anchors.centerIn: parent
                width: 112
                height: 112
                radius: width / 2
                color: Theme.withAlpha(root.tone, 0.10)
                visible: root.running && !root.hasBytes
                SequentialAnimation on scale {
                    running: root.running && !root.hasBytes
                    loops: Animation.Infinite
                    NumberAnimation {
                        from: 0.86
                        to: 1
                        duration: 1300
                        easing.type: Easing.InOutSine
                    }
                    NumberAnimation {
                        from: 1
                        to: 0.86
                        duration: 1300
                        easing.type: Easing.InOutSine
                    }
                }
            }

            StyledText {
                anchors.centerIn: parent
                visible: root.running && root.hasBytes
                text: Math.floor(100 * (root.shown?.done ?? 0) / Math.max(1, root.shown?.total ?? 1)) + " %"
                font.pixelSize: Theme.font.large + 4
                font.weight: Theme.font.weightSemiBold
                font.features: {
                    "tnum": 1
                }
            }

            MaterialIcon {
                anchors.centerIn: parent
                visible: !root.running || !root.hasBytes
                icon: root.ok ? "check" : root.failed ? "warning" : root.phase === "cancelled" ? "stop_circle" : root.phase === "auth" || root.phase === "ventoy" ? "lock" : DrivesText.icon(Drives.drive)
                size: 44
                color: root.tone
                scale: root.ok || root.failed ? 1 : 0.9
                Behavior on scale {
                    SpringAnim {
                        duration: Theme.anim.slow * 1.5
                    }
                }
            }
        }

        StyledText {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: DrivesText.phase(root.shown)
            font.pixelSize: Theme.font.large
            font.weight: Theme.font.weightSemiBold
            wrapMode: Text.Wrap
        }

        StyledText {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            visible: text !== ""
            text: root.running && root.hasBytes ? DrivesText.progressLine(root.shown) : DrivesText.detail(root.shown)
            color: Theme.colors.textMuted
            wrapMode: Text.Wrap
            font.features: {
                "tnum": 1
            }
        }

        // Step dots for writing: write → check → done.
        Row {
            Layout.alignment: Qt.AlignHCenter
            visible: root.shown?.kind === "write" && root.running && root.phase !== "copy"
            spacing: Theme.spacing.sm

            Repeater {
                model: ["write", "verify", "finish"]

                Rectangle {
                    required property string modelData
                    required property int index
                    readonly property int at: ["unmount", "auth", "start"].includes(root.phase) ? -1 : ["write", "verify", "finish"].indexOf(root.phase)

                    width: index === at ? 18 : 6
                    height: 6
                    radius: 3
                    color: index <= at ? root.tone : Theme.withAlpha(Theme.colors.text, 0.18)
                    Behavior on width {
                        SpringAnim {}
                    }
                    Behavior on color {
                        ColorAnim {}
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacing.sm
            spacing: Theme.spacing.sm

            Item {
                Layout.fillWidth: true
            }

            PillButton {
                visible: root.running && ["write", "verify", "copy"].includes(root.phase)
                text: I18n.tr("Cancel")
                onClicked: Drives.cancel()
            }

            PillButton {
                visible: !root.running && root.ok && ["write", "format"].includes(root.shown?.kind) && Drives.drive !== null && Drives.drive.kind !== "loop"
                icon: "eject"
                text: I18n.tr("Eject")
                onClicked: Drives.eject()
            }

            PillButton {
                visible: !root.running
                style: "filled"
                text: root.failed ? I18n.tr("Close") : I18n.tr("Done")
                onClicked: Drives.dismissJob()
            }
        }
    }
}
